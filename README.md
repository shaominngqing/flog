# flog

**Flutter 日志查看器 — 终于能看清日志了。**

终端原生、跨平台、智能的 Flutter 日志查看器。一条命令，零配置。

```bash
cargo install flog
flog
```

## 为什么需要 flog

Flutter 开发中看日志是件痛苦的事：`flutter run` 的控制台输出混乱、没有级别过滤、没有搜索、JSON 挤成一坨、重启就丢失。你不得不在一堆 `I/flutter` 里肉眼找关键信息。

**flog 解决这些问题。** 它是一个独立的日志查看器，常驻后台，自动连接你的 Flutter 应用，给你一个清晰、可交互、可过滤的日志界面。

## 核心卖点

**智能解析，不只是文本**
- 自动识别多种日志格式：`[LEVEL][Tag] message`、Flutter 标准输出、关键词推断
- JSON 自动格式化，支持展开/折叠，语法高亮
- 重复日志自动折叠，不刷屏

**交互式过滤，秒级定位**
- 按级别过滤：一键切换 Verbose / Debug / Info / Warning / Error
- 按 Tag 过滤：`+Network` 只看网络，`-Clog` 排除噪音
- 全文搜索：支持正则 `/api.*error/i`，高亮匹配，`n/N` 跳转

**常驻后台，自动重连**
- flog 先启动，`flutter run` 随便重启，自动发现并连接
- 通过 DDS 代理连接，不干扰 `flutter run`
- 断开后自动回到扫描状态，无需手动操作

**鼠标友好的 TUI**
- 点击选中，双击查看详情，右键书签
- 工具栏可点击：搜索、过滤、级别切换
- 时间线热力图，一眼看出日志分布

**会话持久化**
- 过滤器、书签、搜索条件跨会话保存
- 重启 flog 不丢失状态

## 快速开始

### 推荐工作流

```bash
# 终端 1：启动 flog（等待 Flutter 应用）
flog

# 终端 2：正常开发
flutter run
```

flog 自动发现运行中的 Flutter VM，1-2 秒内连接。`flutter run` 可以随时停止重启，flog 自动重连。

### 其他用法

```bash
# 连接指定 VM Service
flog --uri ws://127.0.0.1:8181/TOKEN=/ws

# Android ADB 模式
flog --adb
flog --adb -s emulator-5554

# 管道模式
flutter run 2>&1 | flog --stdin

# 启动时指定过滤
flog --level w --tag Network
```

## AuraLogger 集成

搭配 [AuraLogger](https://pub.zhenguanyu.com/#/packages/aura_logger) 使用效果最佳。AuraLogger 输出结构化格式：

```
[INFO][Network] -> GET /api/scene-types
[DEBUG][Network]   query: {_productId: 66000001}
[ERROR][SessionCoord] Reconnection failed: timeout
```

flog 原生解析这种格式，精确提取级别、标签、消息内容。任何 Flutter 应用只需引入 AuraLogger 即可获得最佳日志体验。

### 日志级别规范

| 级别 | 用途 | 示例 |
|------|------|------|
| **INFO** | 业务里程碑 | 连接成功、开始练习、评分结果 |
| **DEBUG** | 内部状态 | WS 协议细节、音频状态、Token 缓存 |
| **WARNING** | 可恢复问题 | 会话过期、Token 刷新失败 |
| **ERROR** | 异常/失败 | 连接断开、解析错误、重连失败 |

## 键盘快捷键

| 按键 | 功能 |
|------|------|
| `/` | 搜索（文本或 `/正则/i`） |
| `n` / `N` | 下一个 / 上一个匹配 |
| `j/k` 或 `上/下` | 滚动 |
| `PgUp/PgDn` | 翻页 |
| `Home/End` | 跳到顶部/底部 |
| `Enter` | 打开/关闭详情面板（JSON 格式化） |
| `c` | 复制当前日志到剪贴板 |
| `e` | 导出过滤后的日志到文件 |
| `?` | 帮助 |
| `S` | 统计视图 |
| `Esc` | 关闭面板 / 清除所有过滤 |
| `q` / `Ctrl+C` | 退出 |

## 鼠标操作

| 操作 | 效果 |
|------|------|
| 单击行 | 选中 |
| 双击 | 打开详情 |
| 右键 | 切换书签 |
| 滚轮 | 滚动 |
| 点击工具栏 | 搜索、过滤、切换级别 |
| 点击数据源名称 | 切换数据源 |

## 架构

```
src/
├── domain/     — 核心类型（LogEntry, LogLevel, LogStore, FilterState）
├── input/      — 数据源抽象（ADB, VM Service, stdin, 自动发现）
├── parser/     — 多策略格式检测（AuraLogger, Generic, Keyword）
└── ui/         — 终端界面（ratatui + crossterm）
```

## 技术特性

- **10 万条日志环形缓冲** — 高吞吐不卡顿，超限自动淘汰旧日志
- **多策略解析链** — AuraLogger 结构化格式优先，Generic 兜底，Keyword 推断
- **DDS 代理连接** — 不占用 VM Service WebSocket，不影响 `flutter run`
- **异步架构** — tokio 驱动，数据源、UI、事件处理完全异步
- **Catppuccin Macchiato 主题** — 护眼深色主题，级别颜色区分度高

## License

MIT

---

[English](README_EN.md)
