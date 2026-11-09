# Third-Party Notices / 第三方组件声明

本项目的部分功能移植或派生自以下开源项目，依照其开源协议在此致谢并声明。

---

## DouyinLiveRecorder

- 项目地址：<https://github.com/ihmily/DouyinLiveRecorder>
- 协议：MIT License（Copyright (c) 2025 Hmily）

**移植内容**：

- `src-tauri/src/resolver/ab_sign.rs` —— 抖音 a_bogus 签名算法，逐行移植自原项目
  `src/ab_sign.py`（SM3 哈希、RC4、定制 base64 字符表及字节装配逻辑）
- `src-tauri/src/resolver/huya.rs` —— 虎牙 `wsSecret` 防串签算法，移植自原项目
  `src/stream.py` 的 `get_anti_code`
- `src-tauri/src/resolver/bilibili.rs`、`kuaishou.rs`、`douyin.rs` —— 各平台直播源
  解析的 API 端点、参数与页面解析方式参考/移植自原项目 `src/spider.py`、`src/stream.py`
- 录制状态机行为（循环监测、开播录制、下播续录、分段与无损转码）参照原项目
  `main.py` 的行为重新实现

```
MIT License

Copyright (c) 2025 Hmily

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

---

## FFmpeg

- 项目地址：<https://ffmpeg.org>
- 本项目打包的 Windows 二进制来自第三方构建（如 gyan.dev / BtbN FFmpeg Builds），
  采用 **GPL** 或 LGPL 授权。

**分发说明**：若您再分发包含 FFmpeg 二进制的安装包，请依据其许可（GPL v3）
在您的发布渠道中一并说明并提供对应源代码获取途径（例如附带上游构建页链接：
<https://www.gyan.dev/ffmpeg/builds/> 或 <https://github.com/BtbN/FFmpeg-Builds/releases>）。
仅将 FFmpeg 用于本地录制不产生额外义务。

---

## 前端组件

- UI 组件层基于 [shadcn/ui](https://ui.shadcn.com) 的组件模式编写（MIT）
- [Radix UI Primitives](https://www.radix-ui.com)（MIT）
- [TailwindCSS](https://tailwindcss.com)（MIT）、[lucide-react](https://lucide.dev)（ISC）、
  [sonner](https://sonner.emilkowal.ski)（MIT）等，详见 `package.json`
