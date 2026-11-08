# LiveRecorder 直播录制

基于 **Tauri 2 + Rust + React** 的多平台直播录制桌面应用。循环值守监控直播间，主播开播自动用 ffmpeg 录制，支持画质选择、分段录制、自动转 mp4，内置视频库在线播放。

![架构](https://img.shields.io/badge/Tauri-2-blue) ![Rust](https://img.shields.io/badge/Rust-stable-orange) ![平台](https://img.shields.io/badge/平台-Windows-green)

## 支持平台

| 平台 | 链接形式 | 说明 |
| --- | --- | --- |
| 抖音 | `https://live.douyin.com/房间号` | a_bogus 签名已移植为纯 Rust 实现（SM3+RC4），页面解析兜底；暂不支持短链/主页链接 |
| B站 | `https://live.bilibili.com/房间号` | 官方开放 API |
| 虎牙 | `https://www.huya.com/房间号或主播名` | 移动端 API + wsSecret 签名纯 Rust 实现 |
| 快手 | `https://live.kuaishou.com/u/主播名` | 页面 `__INITIAL_STATE__` 解析 |

架构上每个平台是 `src-tauri/src/resolver/` 下的一个独立模块，实现 `resolve(url, quality, cookie, proxy) -> RoomState` 即可接入新平台。

## 功能

- **直播间管理**：增删改查、启停开关（即时生效）、按房间指定画质
- **自动录制**：循环监测开播状态 → ffmpeg 直录（ts/mkv/flv/mp4）→ 播断自动续录
- **分段录制**：按设定时长切分文件，降低意外损坏损失
- **自动转码**：ts/flv 录完自动无损转 mp4（`-c copy`），可在播放器/浏览器直接播放
- **视频库**：按平台/主播/日期浏览录像，内嵌播放器在线播放、另存为、删除、打开所在文件夹
- **仪表盘**：监测数/录制中文件/磁盘剩余实时展示，运行日志实时滚动
- **单实例**：重复启动自动聚焦已开窗口

## 开发

前置要求：Node 18+、Rust stable（MSVC 工具链）、WebView2（Win10/11 自带）。ffmpeg 已内置在 `src-tauri/binaries/`。

```bash
npm install
npm run tauri dev      # 开发调试（自动编译 Rust + 启动窗口）
npm run tauri build    # 产出 NSIS 安装包 + 绿色版 exe（src-tauri/target/release/bundle/）
```

## 测试

```bash
cargo test                          # 单元测试（ffmpeg 命令快照、文件名清洗）+ ab_sign 对拍测试
cargo test -- --ignored --nocapture # 真实网络测试（四平台解析 + B站 8 秒实录制闭环）
```

- `ab_sign_vectors.rs`：抖音签名对拍向量由 `tests/gen_vectors.py` 对原 Python 实现生成，Rust 输出须逐字节一致
- `recording_live.rs`：解析 → ffmpeg 录制 8 秒 → 优雅停止（stdin `q`）→ 无损转 mp4 的全链路验证

## 设计说明

- **录制状态机**与原 DouyinLiveRecorder 对齐：下播 = ffmpeg 进程退出 → 自动重查续录（刚结束 60 秒内用 30 秒快查）；停用/删除房间即时取消对应任务
- **优雅停止**：向 ffmpeg stdin 写 `q` 保证容器索引完整（mp4 不损坏），不用 SIGKILL
- **设置/房间**持久化在 `%APPDATA%/com.ryan.liverecorder/`，录像默认存 `%APPDATA%/com.ryan.liverecorder/downloads`
- 抖音签名算法来自 [DouyinLiveRecorder](https://github.com/ihmily/DouyinLiveRecorder)（MIT）的 `ab_sign.py`，已逐字节对拍移植
