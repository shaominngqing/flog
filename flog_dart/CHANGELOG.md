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
