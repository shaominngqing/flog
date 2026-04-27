import 'dart:async';

import '../flog_net.dart' show flogEnabled, nextNetId, emitNet;
import 'event.dart';

/// Telemetry tee for an SSE event stream. Emits flog_net protocol messages
/// (`req` / `chunk` / `res` / `err`) while passing every event downstream
/// untouched.
///
/// Usage:
///
/// ```dart
/// byteStream
///   .transform(const SseByteDecoder())
///   .transform(const SseLineDecoder())
///   .transform(FlogSseReporter(url: url, method: 'GET'))
///   .map((e) => e.data); // or: .listen(...)
/// ```
///
/// **Tree-shaking**: when [flogEnabled] is `false` (release builds without
/// `FLOG_ENABLED=true`), construction returns a passthrough sink — no
/// `emitNet` calls, no timestamps captured, no per-event counter — so AOT
/// elimination collapses the transformer to an identity.
class FlogSseReporter extends StreamTransformerBase<SseEvent, SseEvent> {
  /// The request URL, recorded in the `req` frame.
  final String url;

  /// The HTTP method (defaults to `GET`), recorded in the `req` frame.
  final String method;

  const FlogSseReporter({required this.url, this.method = 'GET'});

  @override
  Stream<SseEvent> bind(Stream<SseEvent> stream) {
    if (!flogEnabled) {
      // Pure passthrough. No allocations, no emissions.
      return stream;
    }

    final controller = StreamController<SseEvent>(sync: false);
    final id = nextNetId();
    final start = DateTime.now();
    int seq = 0;
    late StreamSubscription<SseEvent> sub;

    void emit(String type, Map<String, dynamic> extra) {
      emitNet(<String, dynamic>{
        'id': id,
        't': type,
        'p': 'sse',
        ...extra,
      });
    }

    // Subscribe eagerly so `req` fires exactly at the moment the pipeline
    // starts, matching v0.7 semantics.
    scheduleMicrotask(() {
      emit('req', {'method': method, 'url': url});
    });

    sub = stream.listen(
      (event) {
        seq++;
        emit('chunk', {'data': event.data, 'seq': seq});
        controller.add(event);
      },
      onError: (Object e, StackTrace st) {
        emit('err', {'error': e.toString()});
        controller.addError(e, st);
      },
      onDone: () {
        final duration = DateTime.now().difference(start).inMilliseconds;
        emit('done', {
          'duration': duration,
          'chunks': seq,
        });
        controller.close();
      },
      cancelOnError: false,
    );

    controller.onCancel = () => sub.cancel();
    return controller.stream;
  }
}
