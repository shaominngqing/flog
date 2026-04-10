# flog_logger

Lightweight structured logger for Flutter. Outputs `[LEVEL][Tag] message` format that [flog](https://github.com/shaomingqing/flog) parses natively.

## Usage

```dart
import 'package:flog_logger/flog_logger.dart';

// One logger per module — tag is fixed at creation.
final log = FlogLogger('Network');

log.i('-> GET /api/users');
log.d('Response: 200 OK (128ms)');
log.w('Retry #2 after timeout');
log.e('Connection refused', error: e, stackTrace: st);
```

Full-word methods also available:

```dart
log.info('-> GET /api/users');
log.debug('Response: 200 OK');
log.warning('Retry #2');
log.error('Connection refused', error: e);
```

## Output

```
[INFO][Network] -> GET /api/users
[DEBUG][Network] Response: 200 OK (128ms)
[WARNING][Network] Retry #2 after timeout
[ERROR][Network] Connection refused
```

## With flog

Start [flog](https://github.com/shaomingqing/flog) in a separate terminal — it auto-discovers your Flutter app and displays these logs with level coloring, tag filtering, search, bookmarks, and more.

```bash
# Install flog
curl -fsSL https://raw.githubusercontent.com/shaomingqing/flog/master/install.sh | bash

# Run it
flog
```

## License

MIT
