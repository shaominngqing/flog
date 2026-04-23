## 0.7.2

- `flogEnabled` 默认值新增对 `--dart-define=APP_FLAVOR` 的支持：
  - `APP_FLAVOR=release` → 关闭（tree-shake）
  - `APP_FLAVOR=alpha` 或其他 → 开启
  - 未设置 `APP_FLAVOR` 时回落到原逻辑（`!dart.vm.product`）
- 显式 `--dart-define=FLOG_ENABLED=...` 仍然优先生效。

## 0.7.1

- **FlogHttpInterceptor** — `onError` now emits HTTP status code, response headers, and response body for server error responses (4xx/5xx). Previously only a generic error string was sent, causing flog to show "failed" instead of the actual status code.

## 0.2.0

- **FlogHttpInterceptor** — Dio interceptor for HTTP request/response logging to flog Network Inspector.
- **FlogSseParser** — SSE stream wrapper with chunk-level logging.
- **FlogWebSocket** — WebSocket wrapper with send/recv message logging.
- Shared `emitNet()` helper using `[INFO][flog_net]` protocol.
- All interceptors configurable (headers, body, max size, filter).

## 0.1.0

- Initial release.
- `FlogLogger` class with tag-based structured logging.
- Full-word methods: `verbose()`, `debug()`, `info()`, `warning()`, `error()`.
- Single-letter shorthand: `v()`, `d()`, `i()`, `w()`, `e()`.
- Error and stack trace support via named parameters.
