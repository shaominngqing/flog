import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

/// Stream transformer that decodes a raw byte stream into UTF-8 text chunks,
/// handling multi-byte boundary splits that land mid-sequence.
///
/// Design:
///
/// * Stateful per-subscription: a fresh `_ByteDecoderSink` is allocated for
///   each subscription so the transformer instance itself can be reused.
/// * Zero-copy in the hot path: bytes are appended into a `BytesBuilder`
///   which reuses an internal buffer; on boundary split we retain only the
///   trailing 1–3 continuation bytes via `Uint8List.sublistView`.
/// * BOM-strip: a leading UTF-8 BOM (`EF BB BF`) is dropped from the very
///   first decoded chunk.
/// * Bounded: if unflushable bytes accumulate past [maxBufferBytes] the sink
///   emits a descriptive `StateError` and stops forwarding data.
///
/// This replaces the `utf8.decode(byteBuffer)` + fallback-3-bytes loop that
/// previously lived inline in `FlogSseParser._run`.
class SseByteDecoder extends StreamTransformerBase<List<int>, String> {
  /// Hard cap on retained unflushable bytes. Default 1 MiB. If a single SSE
  /// line grows past this without a boundary, the decoder treats it as a
  /// runaway stream and errors out rather than blowing the heap.
  final int maxBufferBytes;

  const SseByteDecoder({this.maxBufferBytes = 1024 * 1024});

  @override
  Stream<String> bind(Stream<List<int>> stream) {
    final controller = StreamController<String>(sync: false);
    late StreamSubscription<List<int>> sub;

    final state = _ByteDecoderState(maxBufferBytes: maxBufferBytes);

    void flushOnError(Object err, StackTrace st) {
      controller.addError(err, st);
    }

    sub = stream.listen(
      (chunk) {
        if (chunk.isEmpty) return;
        try {
          final decoded = state.feed(chunk);
          if (decoded.isNotEmpty) controller.add(decoded);
        } catch (e, st) {
          flushOnError(e, st);
          sub.cancel();
          controller.close();
        }
      },
      onError: (Object e, StackTrace st) {
        flushOnError(e, st);
      },
      onDone: () {
        // On done, attempt a final flush — any leftover bytes at this point
        // are an incomplete UTF-8 sequence (the upstream closed mid-rune).
        try {
          final tail = state.finish();
          if (tail.isNotEmpty) controller.add(tail);
        } catch (e, st) {
          flushOnError(e, st);
        }
        controller.close();
      },
      cancelOnError: false,
    );

    controller.onCancel = () => sub.cancel();
    return controller.stream;
  }
}

/// Internal per-subscription state. Not exposed.
class _ByteDecoderState {
  final int maxBufferBytes;
  final BytesBuilder _builder = BytesBuilder(copy: false);
  bool _bomChecked = false;

  _ByteDecoderState({required this.maxBufferBytes});

  /// Feed raw bytes. Returns the decoded (possibly empty) UTF-8 string that
  /// could be flushed this call; the rest is retained for the next call.
  String feed(List<int> chunk) {
    _builder.add(chunk);
    if (_builder.length > maxBufferBytes) {
      throw StateError(
        'SseByteDecoder: buffer exceeded $maxBufferBytes bytes without a '
        'valid UTF-8 boundary (got ${_builder.length}). The upstream '
        'producer is likely emitting a non-terminated line or malformed UTF-8.',
      );
    }

    // Snapshot the accumulated bytes without copying.
    final bytes = _builder.toBytes();

    // Try a fast, lenient full decode first.
    final decoded = _tryDecode(bytes);
    if (decoded != null) {
      _builder.clear();
      return _maybeStripBom(decoded);
    }

    // Fast path failed — a multi-byte sequence is split across the end of
    // the buffer. A UTF-8 code point is at most 4 bytes, so the tail we
    // must retain is at most 3 bytes. Walk back up to 3 bytes looking for
    // a decodable prefix.
    for (int back = 1; back <= 3 && back <= bytes.length; back++) {
      final prefixLen = bytes.length - back;
      final prefix = Uint8List.sublistView(bytes, 0, prefixLen);
      final prefixDecoded = _tryDecode(prefix);
      if (prefixDecoded == null) continue;
      // Prefix is valid UTF-8 — retain the trailing [back] continuation
      // bytes and emit the prefix.
      final tail = Uint8List.sublistView(bytes, prefixLen);
      _builder.clear();
      _builder.add(tail);
      return _maybeStripBom(prefixDecoded);
    }

    // No valid prefix found — the entire buffer is an incomplete sequence.
    // Keep waiting for more bytes. Nothing to emit this call.
    return '';
  }

  /// Called when upstream closes. If bytes remain, attempt one last decode;
  /// surface residual invalid sequences as a [FormatException].
  String finish() {
    if (_builder.isEmpty) return '';
    final bytes = _builder.toBytes();
    _builder.clear();
    final decoded = _tryDecode(bytes);
    if (decoded != null) return _maybeStripBom(decoded);
    // Leftover incomplete bytes at end of stream.
    throw FormatException(
      'SseByteDecoder: stream closed with ${bytes.length} trailing bytes '
      'that do not form a valid UTF-8 sequence',
    );
  }

  String _maybeStripBom(String s) {
    if (_bomChecked) return s;
    _bomChecked = true;
    if (s.isNotEmpty && s.codeUnitAt(0) == 0xFEFF) {
      return s.substring(1);
    }
    return s;
  }

  static String? _tryDecode(List<int> bytes) {
    if (bytes.isEmpty) return '';
    try {
      return utf8.decode(bytes);
    } on FormatException {
      return null;
    }
  }
}
