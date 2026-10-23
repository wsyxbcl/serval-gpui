<p align="center"><img src="assets/icons/io.github.wsyxbcl.waxbill-lockup.png" alt="Waxbill" width="360"></p>

# Waxbill

Waxbill 是 [Serval](https://github.com/wsyxbcl/Serval) 的桌面界面，用于整理和分析红外相机数据：读取标签、计算独立捕获、
提取媒体、批量修改 XMP 等。每个问题都是表单中的选项，运行前会先读取你的数据，并给出将要执行的完整 Serval 命令。

*English: Waxbill is the desktop app for Serval, the camera-trap data tool. See [English](#english) below.*

## 安装

1. 下载对应平台的 Waxbill 和 Serval。
2. 解压，把 `serval.exe`（Linux / macOS 为 `serval`）和 `waxbill.exe` 放在**同一个文件夹**。
3. 运行 Waxbill。它会自动找到旁边的 Serval；也可以在「设置」中选择其他 Serval 程序。

需要 **Serval 0.8.0 或以上**，建议 **0.8.2**。Windows 可能提示「未知发布者」，选择「仍要运行」即可。

## 功能

| 分组 | 界面 | 做什么 |
| --- | --- | --- |
| 数据分析 | 标签获取 | 读取照片、视频（或 XMP sidecar）中的标签，生成标签表（tags.csv）和物种统计 |
| | 独立捕获 | 按时间间隔合并连续拍到的记录，统计每个位点、每个物种的独立捕获 |
| | 查看媒体 | 按物种和评分筛选，用硬链接在资源管理器中查看；或交给 Caracal；或把改好的表写回 XMP |
| 元数据 | XMP 初始化 | 为每个媒体文件建立 XMP sidecar，并生成时间、GPS 信息表 |
| | XMP 更新 | 按 `xmp_update` 列修改标签，或把整张表写入 XMP（先预览） |
| | 元数据同步 | 把 XMP sidecar 中的标签写入媒体文件 |
| 文件导出 | 媒体提取 | 按物种、评分筛选，把媒体连同 XMP 复制到新文件夹 |
| | XMP 导出 | 按原目录结构复制 XMP sidecar |
| 数据整理 | 重命名 | 把位点文件夹统一改名为 `<位点>_<样区>` |
| | 路径对齐 | 按位点表整理目录结构，或把文件夹摊平 |
| 工具 | XMP 删除 | 删除文件夹中所有的 XMP sidecar |
| | 物种名替换 | 按物种名录替换表中的物种名 |

每个界面都有帮助（F1）。结果默认放在输入文件夹旁边的 `<名称>_serval_output` 里，不会写进数据文件夹。

## 与 Caracal 配合

标签表、独立捕获和 XMP 初始化的结果可以直接「在 Caracal 中打开」（时序分析、物种分析、查看媒体）。开启设置中的
「允许 Caracal 从本机读取所选表格和媒体」后，Caracal 无需再手动选择文件。

## 数据与隐私

Waxbill 不上传任何数据，也不检查更新。唯一的网络服务是给 Caracal 用的本机数据源：只监听 `127.0.0.1`，只提供「在 Caracal 中打开」过的
表格及其中列出的文件，可以在设置中关闭。

## 从源码构建

```sh
cargo build --release
```

Linux 需要 fontconfig、freetype、Wayland、xkbcommon 和 xcb 的开发包（Debian / Ubuntu：`libfontconfig1-dev libfreetype-dev
libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev libx11-xcb-dev pkg-config cmake`）。

设计说明见 [`Design/`](Design/README.md)，界面帮助文本在 [`help/`](help/)。

## English

Waxbill is a desktop front end for [Serval](https://github.com/wsyxbcl/Serval), the command-line tool for camera-trap
data: reading tags, independent captures, extracting media, editing XMP sidecars. Every Serval question is a form
field, the input is read before the run, and the exact Serval command is always shown.

Put `serval` next to `waxbill` (Serval 0.8.0 or later; 0.8.2 recommended) and start Waxbill. The interface is in
Chinese and English. Results go next to the input folder, never inside it. Nothing is uploaded; the only server is a
read-only loopback source for [Caracal](https://caracal.hinature.cn), which can be switched off in Settings.

## 许可 · License

Apache-2.0，见 [LICENSE](LICENSE) 与 [NOTICE](NOTICE)。图标设计 @timber3 · © 2026 山水自然保护中心。
