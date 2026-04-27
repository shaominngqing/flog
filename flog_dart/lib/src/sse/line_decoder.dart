import 'dart:async';

import 'event.dart';

/// W3C Server-Sent Events line-and-field parser as a proper stream
/// transformer. Consumes decoded UTF-8 text chunks, emits [SseEvent]s on
/// dispatch.
///
/// Design notes:
///
/// * Per-subscription state lives on an internal `_LineDecoderState`. The
///   transformer object itself is stateless, so a single instance can be
///   reused across subscriptions and even used with `const`.
/// * On stream close, the sink flushes any pending partial line and any
///   pending event — the W3C spec defines stream-end as an implicit blank
///   line for dispatch purposes.
/// * Supported spec behavior (preserved verbatim from v0.7
///   `_SseLineParser`):
///   - Line endings: `\n`, `\r\n`, or `\r`.
///   - `data:` multi-line: multiple `data:` fields joined with `\n`.
///   - First space after `:` stripped; subsequent spaces preserved.
///   - `:` comment lines recorded and forwarded via [SseEvent.comments].
///   - `id` persists across events; `event` resets after dispatch.
///   - `retry`: only accepted when it parses as a non-negative integer.
///   - An event with no `data` lines is NOT dispatched (per spec), but any
///     `id` seen during its assembly still persists.
class SseLineDecoder extends StreamTransformerBase<String, SseEvent> {
  const SseLineDecoder();

  @override
  Stream<SseEvent> bind(Stream<String> stream) {
    final controller = StreamController<SseEvent>(sync: false);
    final state = _LineDecoderState();
    late StreamSubscription<String> sub;

    sub = stream.listen(
      (text) {
        state.feed(text, controller.add);
      },
      onError: (Object e, StackTrace st) {
        controller.addError(e, st);
      },
      onDone: () {
        state.flush(controller.add);
        controller.close();
      },
      cancelOnError: false,
    );
    controller.onCancel = () => sub.cancel();
    return controller.stream;
  }
}

/// Internal state. All fields here were closure-captured locals in the v0.7
/// `_SseLineParser`; now they live on an object the transformer owns.
class _LineDecoderState {
  String _lineBuffer = '';
  // True if the last byte flushed was '\r' so a following '\n' should be
  // treated as part of the same line ending, not a new blank line.
  bool _pendingCrLf = false;

  // Current event being assembled.
  final List<String> _dataLines = [];
  String? _eventType;
  String? _lastEventId;
  int? _retry;
  final List<String> _comments = [];
  bool _hasEventFields = false;

  void feed(String text, void Function(SseEvent) onEvent) {
    int i = 0;
    while (i < text.length) {
      final ch = text.codeUnitAt(i);
      if (_pendingCrLf) {
        _pendingCrLf = false;
        if (ch == 0x0A) {
          i++;
          continue;
        }
      }
      if (ch == 0x0A) {
        _flushLine(onEvent);
        i++;
      } else if (ch == 0x0D) {
        _flushLine(onEvent);
        _pendingCrLf = true;
        i++;
      } else {
        _lineBuffer += String.fromCharCode(ch);
        i++;
      }
    }
  }

  void flush(void Function(SseEvent) onEvent) {
    if (_lineBuffer.isNotEmpty) {
      _flushLine(onEvent);
    }
    if (_hasEventFields) {
      _dispatch(onEvent);
    }
  }

  void _flushLine(void Function(SseEvent) onEvent) {
    final line = _lineBuffer;
    _lineBuffer = '';

    if (line.isEmpty) {
      if (_hasEventFields) {
        _dispatch(onEvent);
      } else {
        _comments.clear();
      }
      return;
    }

    if (line.startsWith(':')) {
      var comment = line.substring(1);
      if (comment.startsWith(' ')) {
        comment = comment.substring(1);
      }
      _comments.add(comment);
      return;
    }

    final colonIdx = line.indexOf(':');
    String field;
    String value;
    if (colonIdx < 0) {
      field = line;
      value = '';
    } else {
      field = line.substring(0, colonIdx);
      value = line.substring(colonIdx + 1);
      if (value.startsWith(' ')) {
        value = value.substring(1);
      }
    }

    switch (field) {
      case 'event':
        _eventType = value;
        _hasEventFields = true;
        break;
      case 'data':
        _dataLines.add(value);
        _hasEventFields = true;
        break;
      case 'id':
        if (!value.contains('\x00')) {
          _lastEventId = value;
        }
        break;
      case 'retry':
        final parsed = int.tryParse(value);
        if (parsed != null && parsed >= 0) {
          _retry = parsed;
        }
        break;
      default:
        break;
    }
  }

  void _dispatch(void Function(SseEvent) onEvent) {
    if (_dataLines.isEmpty) {
      _eventType = null;
      _retry = null;
      _comments.clear();
      _hasEventFields = false;
      return;
    }
    final data = _dataLines.join('\n');
    final event = SseEvent(
      event: _eventType,
      data: data,
      id: _lastEventId,
      retry: _retry,
      comments: _comments.isEmpty ? null : List.unmodifiable(_comments),
    );
    onEvent(event);

    _dataLines.clear();
    _eventType = null;
    _retry = null;
    _comments.clear();
    _hasEventFields = false;
    // _lastEventId persists across events per spec.
  }
}
