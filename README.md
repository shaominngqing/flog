# flog

```
███████╗██╗      ██████╗  ██████╗
██╔════╝██║     ██╔═══██╗██╔════╝
█████╗  ██║     ██║   ██║██║  ███╗
██╔══╝  ██║     ██║   ██║██║   ██║
██║     ███████╗╚██████╔╝╚██████╔╝
╚═╝     ╚══════╝ ╚═════╝  ╚═════╝
```

**Flutter 开发日志，不该这么难看。**

```bash
curl -fsSL https://raw.githubusercontent.com/shaominngqing/flog/master/install.sh | bash
```

## 你现在看日志有多痛苦

`flutter run` 的终端输出是这样的：

```
I/flutter (12345): [INFO][Network] → POST /api/chat/prompt
I/flutter (12345): [DEBUG][Network]   body: {messages: [{role: assistant, content: Welcome}, {role: user, content: I wanna learn about...}], stream: true}
I/flutter (12345): [INFO][Network] ← 201 /api/chat/prompt (1140ms)
I/flutter (12345): [DEBUG][Network]   body: Instance of 'ResponseBody'
W/1.raster(12345): type=1400 audit(0.0:574): avc: denied { read }
D/TrafficStats(12345): tagSocket(194) with statsTag=0xffffffff
I/flutter (12345): [DEBUG][GoalRepo] promptChat response: Great choices! Those are super practical...
I/flutter (12345): [INFO][Clog] /network/request {userId: unknown, path: /api/chat/prompt, status: 201}
```

**一坨。** 系统日志和业务日志混在一起，没有颜色区分，没法过滤，没法搜索，JSON 挤成一行，重启就全丢了。

打开 DevTools 的 web 页面？加载慢、界面复杂、每次 hot restart 断连要重新打开、而且看不到 `print()` 输出。

## flog 给你的

同样的日志，在 flog 里是这样的：

```
17:34:43.710  INFO     Network     → POST /api/chat/prompt
17:34:43.711  DEBUG    Network       body: {messages: [...], stream: true}
17:34:44.353  INFO     Network     ← 201 /api/chat/prompt (1140ms)
17:34:44.360  DEBUG    Network       body: Instance of 'ResponseBody'
17:34:44.368  INFO     Clog        /network/request {path: /api/chat/prompt, status: 201}
17:35:02.186  DEBUG    GoalRepo    promptChat response: Great choices!...
```

**干净。** 只有你的业务日志。级别颜色区分。Tag 对齐。系统噪音自动过滤。

## 不只是好看

**按级别一键过滤** — 工具栏点一下，只看 Warning + Error，瞬间定位问题

**按 Tag 精准过滤** — `+Network` 只看网络请求，`-Clog` 排除埋点噪音，支持正则

**全文搜索** — `/timeout/i` 搜索所有超时日志，`n/N` 在匹配间跳转，高亮显示

**JSON 展开/折叠** — 双击一条日志，右侧面板里 JSON 自动格式化、语法高亮、点击折叠

**常驻后台，自动重连** — flog 开着不用管，`flutter run` 随便重启，1-2 秒自动连上。不像 DevTools 每次都要重新打开

**10 万条日志不卡** — 环形缓冲 + 异步架构，高频日志照样丝滑滚动

**书签 + 导出** — 右键标记关键日志，`e` 一键导出过滤结果

## 30 秒上手

```bash
# 终端 1
flog

# 终端 2
flutter run
```

没了。flog 自动发现你的 Flutter 应用，通过 DDS 代理连接（不影响 `flutter run`），日志实时显示。

### 更多用法

```bash
flog --adb                    # Android ADB logcat 模式
flog --adb -s emulator-5554   # 指定设备
flog --level w                # 只看 Warning 以上
flog --tag Network            # 只看 Network 标签
flutter run 2>&1 | flog --stdin  # 管道模式
```

## 搭配 AuraLogger 效果最佳

flog 原生解析 [AuraLogger](https://pub.zhenguanyu.com/#/packages/aura_logger) 的结构化格式，精确提取级别、标签、消息。你的 Flutter 项目只需：

```yaml
dependencies:
  aura_logger:
    hosted: https://pub.zhenguanyu.com
    version: ^0.0.3
```

```dart
AuraLogger.i('-> GET /api/users', tag: 'Network');
AuraLogger.e('Connection failed: $e', tag: 'WS');
```

没有 AuraLogger 也能用 — flog 自动识别 Flutter 标准输出格式和常见日志模式。

## 键盘 & 鼠标

| 操作 | 效果 |
|------|------|
| `/` | 搜索（支持 `/正则/i`） |
| `n` / `N` | 下一个/上一个匹配 |
| `j/k` 或方向键 | 滚动 |
| `Enter` | 打开详情面板 |
| 双击 | 打开详情 |
| 右键 | 切换书签 |
| `e` | 导出日志 |
| `S` | 统计视图 |
| `?` | 帮助 |
| `q` | 退出 |

## 安装

```bash
# 一键安装（推荐）
curl -fsSL https://raw.githubusercontent.com/shaominngqing/flog/master/install.sh | bash

# 或通过 Cargo
cargo install flog
```

支持 macOS (Intel/Apple Silicon)、Linux (x86_64/aarch64)、Windows。

## License

MIT

---

[English](README_EN.md)
