import 'dart:async';
import 'dart:convert';

import 'flog_net.dart' show flogEnabled, nextNetId, emitNet;

/// Wraps an SSE (Server-Sent Events) byte stream and emits flog_net protocol
/// messages for each event, while forwarding the parsed data downstream.
///
/// Handles `data:` prefixed lines, `[DONE]` terminators, and incomplete
/// UTF-8 byte sequences across chunk boundaries.
///
/// ```dart
/// final response = await dio.get<ResponseBody>(url,
///   options: Options(responseType: ResponseType.stream));
/// final sseStream = FlogSseParser.wrap(
///   response.data!.stream,
///   url: url,
/// );
/// await for (final event in sseStream) {
///   // process SSE event data
/// }
/// ```
class FlogSseParser {
  FlogSseParser._();

  /// Wrap a raw byte stream from an SSE endpoint.
  ///
  /// Returns a [Stream<String>] of parsed SSE data payloads. Protocol events
  /// are emitted to flog_net automatically.
  static Stream<String> wrap(
    Stream<List<int>> byteStream, {
    required String url,
    String method = 'GET',
  }) {
    if (!flogEnabled) {
      return _passThroughSse(byteStream);
    }

    final id = nextNetId();
    int seq = 0;
    int totalSize = 0;
    final start = DateTime.now();

    // Buffer for incomplete UTF-8 sequences
    final byteBuffer = <int>[];
    // Buffer for incomplete SSE lines
    String lineBuffer = '';

    final controller = StreamController<String>();

    void emitEvent(String type, Map<String, dynamic> extra) {
      final data = <String, dynamic>{
        'id': id,
        't': type,
        'p': 'sse',
        ...extra,
      };
      emitNet(data);
    }

    emitEvent('req', {
      'method': method,
      'url': url,
    });

    final subscription = byteStream.listen(
      (chunk) {
        totalSize += chunk.length;

        // Append to byte buffer and attempt UTF-8 decode
        byteBuffer.addAll(chunk);
        String decoded;
        try {
          decoded = utf8.decode(byteBuffer);
          byteBuffer.clear();
        } on FormatException {
          // Incomplete UTF-8 sequence; wait for more bytes.
          // Try to decode as much as possible.
          // Walk backwards to find a safe boundary (max 4 bytes for UTF-8)
          for (int i = 1; i <= 4 && i <= byteBuffer.length; i++) {
            try {
              decoded = utf8.decode(byteBuffer.sublist(0, byteBuffer.length - i));
              final remaining = byteBuffer.sublist(byteBuffer.length - i);
              byteBuffer
                ..clear()
                ..addAll(remaining);
              // Process the safely decoded portion
              _processDecoded(decoded, lineBuffer, (newBuffer, data) {
                lineBuffer = newBuffer;
                if (data != null) {
                  seq++;
                  emitEvent('chunk', {
                    'data': data,
                    'seq': seq,
                  });
                  controller.add(data);
                }
              });
              return;
            } on FormatException {
              continue;
            }
          }
          // If nothing could be decoded, just wait for more data
          return;
        }

        _processDecoded(decoded, lineBuffer, (newBuffer, data) {
          lineBuffer = newBuffer;
          if (data != null) {
            seq++;
            emitEvent('chunk', {
              'data': data,
              'seq': seq,
            });
            controller.add(data);
          }
        });
      },
      onError: (Object error) {
        emitEvent('err', {
          'error': error.toString(),
        });
        controller.addError(error);
      },
      onDone: () {
        final duration = DateTime.now().difference(start).inMilliseconds;
        emitEvent('done', {
          'duration': duration,
          'chunks': seq,
          'size': totalSize,
        });
        controller.close();
      },
      cancelOnError: false,
    );

    controller.onCancel = () {
      subscription.cancel();
    };

    return controller.stream;
  }

  /// Pass-through SSE parser that does no flog_net emission.
  ///
  /// Used when [flogEnabled] is false so the stream is still parsed into
  /// SSE data payloads but no protocol messages are emitted.
  static Stream<String> _passThroughSse(Stream<List<int>> byteStream) {
    final byteBuffer = <int>[];
    String lineBuffer = '';
    final controller = StreamController<String>();

    final subscription = byteStream.listen(
      (chunk) {
        byteBuffer.addAll(chunk);
        String decoded;
        try {
          decoded = utf8.decode(byteBuffer);
          byteBuffer.clear();
        } on FormatException {
          for (int i = 1; i <= 4 && i <= byteBuffer.length; i++) {
            try {
              decoded = utf8.decode(byteBuffer.sublist(0, byteBuffer.length - i));
              final remaining = byteBuffer.sublist(byteBuffer.length - i);
              byteBuffer
                ..clear()
                ..addAll(remaining);
              _processDecoded(decoded, lineBuffer, (newBuffer, data) {
                lineBuffer = newBuffer;
                if (data != null) {
                  controller.add(data);
                }
              });
              return;
            } on FormatException {
              continue;
            }
          }
          return;
        }

        _processDecoded(decoded, lineBuffer, (newBuffer, data) {
          lineBuffer = newBuffer;
          if (data != null) {
            controller.add(data);
          }
        });
      },
      onError: (Object error) {
        controller.addError(error);
      },
      onDone: () {
        controller.close();
      },
      cancelOnError: false,
    );

    controller.onCancel = () {
      subscription.cancel();
    };

    return controller.stream;
  }

  /// Process decoded text, calling [onData] for each SSE data event found.
  ///
  /// [onData] receives the updated line buffer and optionally the parsed data
  /// string. If data is `null`, only the buffer was updated.
  static void _processDecoded(
    String decoded,
    String lineBuffer,
    void Function(String newBuffer, String? data) onData,
  ) {
    final combined = lineBuffer + decoded;
    final lines = combined.split('\n');

    // Last element might be incomplete
    final incomplete = lines.removeLast();

    for (final line in lines) {
      final trimmed = line.trim();

      // Empty line = end of SSE event block (we already emitted per data: line)
      if (trimmed.isEmpty) continue;

      if (trimmed.startsWith('data:')) {
        final payload = trimmed.substring(5).trim();

        // [DONE] is the conventional SSE stream terminator
        if (payload == '[DONE]') continue;

        onData(incomplete, payload);
        return;
      }
    }

    // No data event found in this chunk, just update the buffer
    onData(incomplete, null);
  }
}
