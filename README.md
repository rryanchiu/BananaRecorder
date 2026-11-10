<div align="center">
  <img src="public/banana.png" width="80" alt="BananaRecorder logo" />
  <h1>BananaRecorder · 香蕉录制</h1>
  <p><strong>直播开播，自动记录。</strong><br />为 Windows 打造的多平台直播监控与录制桌面应用。</p>
  <p>
    <img src="https://img.shields.io/badge/platform-Windows-0078D4" alt="Windows" />
    <img src="https://img.shields.io/badge/Tauri-2-24C8D8" alt="Tauri 2" />
    <img src="https://img.shields.io/badge/Rust-native-CE422B" alt="Rust backend" />
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-green" alt="MIT license" /></a>
  </p>
  <p><strong>简体中文</strong> · <a href="README.en.md">English</a></p>
  <p><a href="#功能亮点">功能</a> · <a href="#快速开始">快速开始</a> · <a href="#本地开发">开发</a> · <a href="#构建安装包">打包</a> · <a href="#开源与致谢">开源说明</a></p>
</div>

---

BananaRecorder 将直播间管理、开播监控、FFmpeg 录制和视频整理放进一个桌面窗口。前端使用 React，后端由 Rust 驱动，无需启动 Python 服务。

> 当前版本为 **0.1.0**，面向 Windows。界面目前为简体中文；英文 README 不代表应用已有英文界面。平台接口可能变化，支持列表表示已实现对应解析器，不保证所有直播间始终可用。

## 功能亮点

| 能力 | 说明 |
| --- | --- |
| 多平台管理 | 集中管理抖音、B站、虎牙与快手直播间，单独启停、设置画质 |
| 自动值守 | 定时检查开播状态，发现直播后自动录制，流中断后重新检查 |
| 灵活录制 | TS / MKV / FLV / MP4 输出，按时长分段，按主播和日期整理目录 |
| 录后处理 | 自动转为 MP4，可选择删除原文件；视频库支持批量转换与合并 |
| 视频库 | 浏览、预览、另存为、删除录像，打开所在文件夹 |
| 实时仪表盘 | 查看监控数量、录制任务、剩余磁盘空间与运行日志 |
| 桌面体验 | 系统托盘、单实例运行、自定义窗口栏与应用内滚动 |
| 连接设置 | 支持抖音/B站 Cookie 配置及代理设置 |

## 支持平台

请使用直播间网页地址，而非分享短链或普通用户主页。

| 平台 | 地址示例 | 备注 |
| --- | --- | --- |
| 抖音 | `https://live.douyin.com/房间号` | 暂不支持短链及普通主页链接 |
| 哔哩哔哩 | `https://live.bilibili.com/房间号` | 可在设置中配置 Cookie |
| 虎牙 | `https://www.huya.com/房间号或主播名` | 实际画质取决于直播源 |
| 快手 | `https://live.kuaishou.com/u/主播名` | 依赖直播页面解析，可能受访问限制影响 |

## 快速开始

### 安装与启动

如果当前仓库提供 Release，请选择 Windows 安装程序；尚无发布包时，可按下文从源码构建。安装包比单独复制主程序更适合分发，因为录制还需要 FFmpeg。

运行环境：Windows 10 / 11，以及 Microsoft Edge WebView2 Runtime。开发与打包目前以 Windows 为目标，尚未提供 macOS / Linux 的验证说明。

### 第一次录制

1. 打开 **录制设置**，选择保存目录，确认画质、录制格式和分段选项。
2. 在 **直播间管理** 中添加直播间链接，并打开该房间的启用开关。
3. 点击右上角 **开始监控**。直播间开播后，应用自动开始录制。
4. 在 **仪表盘** 查看状态和日志，在 **视频库** 查看生成的录像。

**监控与停止行为：**

- **停止监控**：停止全局监控引擎及当前录制任务。
- **单个房间的停止按钮**：停止该房间录制，同时停用该房间；重新打开开关可恢复监控。
- **关闭窗口**：隐藏到系统托盘，后台任务继续运行。完全退出请使用托盘菜单。
- **重新启动应用**：若存在已启用的房间，应用会自动启动监控引擎。

### 默认设置

| 设置 | 默认值 |
| --- | --- |
| 画质 | 原画 |
| 输出格式 | TS |
| 检查间隔 | 120 秒 |
| 自动分段 | 开启，每段 30 分钟 |
| 录后转 MP4 | 开启 |
| 转换后删除原文件 | 开启 |
| 目录整理 | 按主播分类 |

请在首次使用时确认这些选项，尤其是转换后删除原文件的设置。实际可用画质取决于平台、账号权限和直播源。

## 数据与隐私

Windows 下，设置与直播间列表默认存放在：

```text
%APPDATA%\com.ryan.liverecorder\
├── settings.json
└── rooms.json
```

未设置保存目录时，录像默认写入该应用数据目录下的 `downloads` 文件夹。更改源码所在目录不会迁移这些用户数据。

Cookie 保存在本地设置 JSON 中，并非加密凭据存储。请勿将设置文件、Cookie、带鉴权参数的直播地址或未经脱敏的日志提交到仓库、Issue 或截图中。

## 本地开发

### 环境准备

- **Node.js 22.12+** 与 npm（当前 Vite 的 Node 要求为 `^20.19.0 || >=22.12.0`）。
- **Rust stable**，使用 Windows MSVC 工具链。
- **Visual Studio Build Tools**，安装 C++ 桌面开发工具和 Windows SDK。
- **Microsoft Edge WebView2 Runtime**。
- **FFmpeg**：准备 Windows 构建，将 `ffmpeg.exe` 放到 `src-tauri/binaries/ffmpeg.exe`。

> FFmpeg 二进制不在 Git 中。即使系统 PATH 已有 FFmpeg，构建安装包前也必须准备上述资源文件。可从 [FFmpeg 下载页](https://ffmpeg.org/download.html) 选择构建，并核对其许可。

### 安装依赖并启动

下载或克隆本仓库后，在项目根目录执行：

```powershell
npm ci
npm run tauri dev
```

应用运行时不依赖原 DouyinLiveRecorder 目录或 Python。`npm run dev` 只启动前端开发服务器；完整桌面功能需要通过 `npm run tauri dev` 使用。

### 常用命令

| 命令 | 用途 |
| --- | --- |
| `npm run tauri dev` | 启动完整桌面开发环境 |
| `npm run build` | TypeScript 检查与前端生产构建 |
| `cargo check --manifest-path src-tauri/Cargo.toml` | 检查 Rust 后端 |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 运行默认测试 |
| `npm run tauri build` | 构建桌面应用与安装包 |

## 构建安装包

在项目根目录执行：

```powershell
npm run tauri build
```

主要输出位置：

```text
src-tauri/target/release/
├── liverecorder.exe       # 主程序，内部文件名仍沿用 liverecorder
└── bundle/
    ├── nsis/             # Windows EXE 安装程序
    └── msi/              # MSI 安装包（对应打包步骤成功时生成）
```

只构建 NSIS 安装程序：

```powershell
npm run tauri build -- --bundles nsis
```

主程序 EXE 不等于完整的独立单文件发行包。若手动分发主程序，需要同时处理 FFmpeg、WebView2 运行环境及第三方许可文件。首次构建可能需要联网下载 Rust 依赖和安装包工具。

## 测试

默认测试包括 FFmpeg 命令构建、文件名清理、签名算法及 Python/Rust 签名向量对拍。联网测试默认跳过。

```powershell
# 默认测试
cargo test --manifest-path src-tauri/Cargo.toml

# 显式执行联网测试：访问真实平台，部分测试会进行短时录制
cargo test --manifest-path src-tauri/Cargo.toml -- --ignored --nocapture
```

联网测试依赖房间当前状态、平台接口和本地网络，不适合作为完全可复现的离线验证。

签名测试向量已随仓库提交；仅当需要重新生成参考向量时，才需要原 Python 项目及其依赖：

```powershell
python src-tauri/tests/gen_vectors.py "D:\path\to\DouyinLiveRecorder"
```

## 项目结构

```text
src/
├── components/           # 窗口布局与共享 UI
├── pages/                # 仪表盘、直播间、视频库、设置
└── lib/                  # Tauri 调用与前端状态
src-tauri/
├── src/
│   ├── resolver/         # 各平台直播源与签名解析
│   ├── recorder/         # 录制调度与 FFmpeg 进程管理
│   ├── commands.rs       # 前后端命令接口
│   ├── config.rs         # 设置持久化
│   └── rooms.rs          # 直播间管理
├── tests/                # 单元/集成测试与参考向量
├── binaries/             # 本地准备的 FFmpeg，不纳入 Git
└── tauri.conf.json       # 窗口与打包配置
```

## 常见问题

**已开始监控，为什么没有录制？**

先确认房间已启用且正在直播，再查看仪表盘日志。地址形式不支持、Cookie 失效、网络或平台限制都可能导致解析失败。

**为什么录像不能在应用内播放？**

内嵌播放器受 WebView2 支持的容器与编码限制。可转为 MP4 或使用本地播放器；仅更换容器并不保证编码兼容。

**找不到 FFmpeg？**

应用依次检查 PATH、开发资源目录、主程序旁的 `ffmpeg.exe` 和安装资源目录。开发时优先确认 `src-tauri/binaries/ffmpeg.exe` 存在。

**修改代码后，已安装的应用为什么没有变化？**

开发模式使用 `npm run tauri dev`；已安装版本需要重新打包并更新安装。

## 参与贡献

欢迎提交问题反馈、平台解析修复和体验改进。报告问题时，请提供应用版本、Windows 版本、复现步骤及脱敏后的相关日志，并注明直播平台。较大的改动建议先通过 Issue 讨论。

提交前请运行前端构建与相关 Rust 测试；修改平台解析器时，说明验证的平台、场景及仍未覆盖的情况。请勿提交 `node_modules`、编译产物、录像或个人配置。

## 开源与致谢

BananaRecorder 的部分签名算法与平台解析逻辑**移植或派生自** [DouyinLiveRecorder](https://github.com/ihmily/DouyinLiveRecorder)（MIT，Copyright © 2025 Hmily），包括抖音 a_bogus 签名、虎牙防串签及部分直播源解析。项目保留相关版权与许可说明，不将这些内容声明为完全原创。

感谢 Tauri、Rust、React、FFmpeg，以及 shadcn/ui、Radix UI、Tailwind CSS、Lucide 和 Sonner。

项目自有代码采用 [MIT License](LICENSE)。第三方代码及二进制仍遵循各自许可，详见 [第三方声明](THIRD-PARTY-NOTICES.md)。当前本地 FFmpeg 为 `6.0-essentials_build-www.gyan.dev`，启用了 GPL/version3；对外分发含 FFmpeg 的安装包前，需核实并提供该构建所需的许可文本及对应源码获取安排。项目 MIT 许可不替代 FFmpeg 的许可要求。

请仅录制和使用你有权保存的内容，并遵守相关平台规则及适用法律。
