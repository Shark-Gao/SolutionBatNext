# MHAutoUpdateCompilerNext

基于 **Tauri 2 + Vue 3 + TypeScript + Rust** 重写的 MHAutoUpdateCompilerNext。
源工程：`K:\SolutionBat\SolutionBat`。新工程与旧工程独立存放。

## 运行

双击 `release\MHAutoUpdateCompilerNext.exe`，或运行根目录的 `Launch.cmd`。
`release` 中同时提供 Windows 当前用户安装包。

运行新版不需要安装 Python、Rust 或 Node.js。实际构建 TypeScript 时仍需要 Node.js，以及被构建项目本身安装的 TypeScript；P4 同步需要本机 `p4.exe`。Windows 需要 Microsoft WebView2 Runtime，安装包会在缺少时引导安装。

配置、界面设置和运行记录索引保存在 exe 同级的 `config` 文件夹，便携版当前为 `K:\SolutionBat\SolutionBatNext\release\config`。升级时首次读取会自动复制 `%LOCALAPPDATA%\SolutionBatNext` 中的已有配置、备份和历史记录，保留工作区编号；旧文件不删除，新位置已有配置时不会覆盖。没有已有配置时才从旧版发布目录的快照初始化四个工作区，不会写入旧版 `config.xml`。WebView 缓存仍使用用户数据目录，不属于工作区配置。运行日志保存在 exe 同级的 `logs` 文件夹。

## 功能

- 工作区新建、复制、重命名、删除、搜索；支持导入旧版 XML / 新版 JSON 和导出 JSON。
- 项目根目录关联 UE 项目、引擎、TS 工程和命令行程序，也可关闭关联后手工配置。
- P4 参数、强制同步、UE 编译配置、Editor/Game/Client/Server 构建目标和平台选择。
- 六项原有任务：关闭 UE、同步 P4、编译 UE、生成并构建 TS、单独生成定义、编译 TS 并启动 Watch。
- 执行预演、环境检查、完整命令查看、实时日志、日志筛选/搜索/复制/导出、运行记录。
- 多工作区运行管理、停止子进程树、同一根目录的跨进程互斥、构建失败中止后续步骤。
- Windows 每日多个触发时间，注册/更新/移除计划任务，查询下次运行时间。
- 计划任务自动打开独立执行窗口，显示对应工作区、阶段状态和实时日志；支持停止，结束后保留结果窗口，不占用下一次定时触发。可在每个工作区的计划任务设置中选择任务成功后打开 Rider 或 Visual Studio，并从本机已安装版本中选择启动版本或指定程序路径。
- 浅色、深色、跟随系统主题；桌面窄窗口和移动浏览器预览布局。
- 官方 Tauri 更新插件接入与独立更新配置模板。
- 原版 iWiki 帮助文档、随程序附带的离线使用手册、关于与作者信息。
- 界面主题即时生效并独立自动保存，不会连带保存未确认的项目配置。
- 原版 P4 可写冲突恢复、最多三轮自动合并、编译前等待 UnrealBuildTool。
- 原版局域网打开/关闭使用记录上报，失败不影响构建。

浏览器预览只操作浏览器内的配置副本，不执行本机任务或写入计划任务。实际能力在桌面程序中提供。

## 与旧版的兼容规则

1. P4 命令显式传入服务器、用户、客户端，同步范围为指定客户端。按用户确认保留旧版自动处理：可写冲突文件强制同步、最多三轮 `resolve -am`；可写文件覆盖限定为当前项目目录。执行前应备份重要本地更改。
2. 手动关闭 UE 与旧版一样匹配 UnrealEditor.exe 和 UnrealEditor-Win64-DebugGame.exe；计划任务先关闭 Rider 和 UE，再开始构建。进程操作均限定为当前 Windows 用户。UE 由 Rider 调试器启动时，同时关闭路径和用户均匹配的父调试器，并确认进程真正退出；未退出时明确中止，不把关闭请求当作关闭成功。
3. 重复勾选 TS 组合任务与独立任务时，生成和 Watch 各执行一次。
4. 手动和定时 TS 构建都与旧版一样启动独立的 TS Watch 窗口，然后结束本次构建流程。关闭该窗口可停止对应 Watch，窗口输出不归入主程序日志。
5. 计划任务不受手动勾选项影响，固定执行：关闭 Rider / UE、更新 P4、编译 UE、生成 TS 定义、启动 TS Watch。手动勾选项不会被改写。
6. 计划任务成功完成全部步骤后，按该工作区计划任务中的“任务成功后打开项目工程”设置，用 Rider 或 Visual Studio 打开当前工作区的 `MHAGame\MHMobile.sln`。版本下拉显示本机实际检测到的安装版本；优先使用指定启动程序路径，再按所选版本查找，最后尝试系统默认启动程序；找不到时记录日志并跳过。失败、停止或预演不会打开项目工程。Rider 的 AI Plan 模式不能通过稳定的启动参数自动切换。
6. 计划任务使用旧版的 `DailyBuild_<工作区名称>`，可直接查看、重新注册或删除原任务。点“注册计划任务”后才将执行程序切换到新版，并清理此前同工作区的 `SolutionBatNext_<UUID>` 兼容任务；启动程序或保存配置不会改动系统任务。重命名前需先删除该工作区任务。
7. 计划任务与旧版一样使用当前登录用户、最高权限运行，支持每天多个触发时间、错过后补运行。注册和删除时需要以管理员身份运行程序；不存储 Windows 密码，用户未登录时不会执行。已存在的任务会显示真实下次运行时间和执行程序。
8. Windows 上的 UE 命令行程序按宿主 Win64 计算，和 Android/iOS 等构建目标分开。
9. 生成类型定义前备份旧 `ue.d.ts`；失败或未生成新文件时恢复旧文件。
10. 按用户确认启用旧版局域网统计协议，仍发送到 `http://10.30.129.88:9876/api/notify`，工具标识 `MHAutoUpdateCompiler`，包含时间、打开/关闭事件、用户、机器名、IP、系统和当前软件版本。打开时异步发送，关闭时至多等待约 1 秒。新版客户端不依赖旧工程的 Python 模块；原服务端无需修改。
11. P4 报告项目内 exe 被占用时，仅对当前用户、执行路径完全匹配的占用进程请求正常关闭，成功后重试同步一次；拒绝关闭时中止并提示，不强制关闭其他工具、不操作项目目录之外的程序。

## 帮助与日志

日志面板可拖动上边缘调整高度，双击恢复默认。最大化、收起后仍可恢复手动高度，并在下次打开时保留；调整高度不会保存未完成的工作区编辑。

设置中的“在线帮助文档”在默认浏览器打开原 iWiki 地址；顶部书本图标和设置中的“离线使用手册”打开随程序安装的说明。设置中的“关于”保留原工具名称、作者和功能说明。

程序目录下的 `logs` 包含 `manual_YYYYMMDD.log`、`scheduled_task_YYYYMMDD.log`、`gui_main_YYYYMMDD.log`，以及每次运行的 JSONL 明细。设置中提供“打开日志目录”。旧版本曾保存在用户数据目录的历史日志仍可读取；新日志不会继续写入那里。移动程序时建议同时移动 `logs` 文件夹。

如果 exe 目录不可写，会报告配置或日志写入错误，不会静默改用其他目录。移动程序时请同时保留 `config` 和 `logs`。`--data-dir` 只指定配置、锁和运行记录索引目录，不改变日志位置。测试通过 `SOLUTIONBAT_DATA_DIR` 显式隔离数据与日志，并使用 `SOLUTIONBAT_DISABLE_TELEMETRY=1` 禁止向真实局域网服务上报；显式隔离目录不会从真实用户配置自动迁移。

## 开发

需要 Windows 10/11、Node.js 22.12+、Rust stable，以及 Visual Studio 的 C++ 桌面开发工具和 Windows SDK。
本机的 Rust 工具链已放在 `.toolchain`；脚本只修改自身进程的环境，不修改系统 PATH。

```powershell
# 新电脑安装依赖；本机已完成
powershell -ExecutionPolicy Bypass -File scripts/setup.ps1

# 启动桌面开发环境
powershell -ExecutionPolicy Bypass -File scripts/dev.ps1

# 编译可执行程序和 NSIS 安装包
powershell -ExecutionPolicy Bypass -File scripts/build.ps1

# 配置逻辑、Rust 后端和浏览器流程测试
powershell -ExecutionPolicy Bypass -File scripts/test.ps1

# 桌面集成测试：仅执行预演和读取系统状态
powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -Debug
node tests/native-smoke.mjs

# 只预览界面
npm run dev
```

浏览器流程测试使用本机 Microsoft Edge。前端服务默认地址 `http://127.0.0.1:1420`。

## 目录

```text
src/                       Vue 界面、类型、前后端桥接
src-tauri/src/model.rs      配置模型与校验
src-tauri/src/storage.rs    XML 迁移、JSON 原子保存、备份
src-tauri/src/plan.rs       任务编排和命令参数
src-tauri/src/runner.rs     进程树、实时日志、取消、历史
src-tauri/src/schedule.rs   Windows 计划任务
src-tauri/src/lib.rs        Tauri 命令与窗口
src-tauri/src/main.rs       桌面入口和计划任务入口
src-tauri/src/monitor.rs    计划任务窗口状态同步与停止请求
src-tauri/fixtures/         本机旧版配置迁移快照
scripts/                   安装依赖、开发、测试、打包
tests/                     前端和真实桌面集成测试
release/                   可执行程序、安装包
```

任务命令定义在独立的 `Step` 列表中，添加任务时扩展 `plan.rs` 和界面任务模型即可。高频运行日志只保留最近 1200 条用于界面显示，每次运行的磁盘日志上限为 32 MiB；运行历史界面显示最近 100 条，旧文件仍留在数据目录。

## GitHub 发布与自动更新

工程可以生成供其他人下载的 Windows 当前用户安装包。仓库中的 `.github/workflows/publish.yml` 采用 GitHub Releases 发布：推送 `app-v*` 标签或手动运行工作流后，会执行测试、构建 NSIS 安装包，并生成 Tauri updater 所需的签名文件和 `latest.json`。

启用自动更新前：

1. 使用 `npm run tauri -- signer generate` 创建 Tauri 更新签名密钥，私钥必须保存在工程之外。
2. 在 GitHub 仓库 Secrets 中设置 `TAURI_SIGNING_PRIVATE_KEY`、`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`（如果密钥没有密码可以留空）和 `TAURI_UPDATER_PUBLIC_KEY`。
3. 修改 `package.json`、`src-tauri/Cargo.toml` 和 `src-tauri/tauri.conf.json` 中的版本号，保持三处一致。
4. 提交代码并推送标签，例如 `app-v0.2.0`。
5. 在 GitHub Releases 中检查草稿发布，确认资产无误后发布。

内置更新地址使用：`https://github.com/Shark-Gao/SolutionBatNext/releases/latest/download/latest.json`。GitHub Release 可以作为静态更新源，首次安装下载 `*-setup.exe`，软件内更新则读取 `latest.json` 并校验签名后安装更新包。它可以承担小规模软件的发布和下载分发；如果后续用户较多，或部分用户访问 GitHub 不稳定，再把同一组更新资产同步到对象存储/CDN 即可，应用端只需调整 endpoint。

Tauri updater 强制要求更新签名：公钥可以放进应用和仓库，私钥不能提交到 GitHub；一旦丢失私钥，已有版本将无法继续验证后续更新。GitHub 仓库若为私有，普通用户无法直接访问 Release 更新地址，建议使用公开 Release 或单独的公开更新镜像。

本地仍可使用 `scripts/build.ps1 -UpdaterConfig src-tauri/tauri.updater.local.json` 生成签名版本。当前没有 Windows 代码签名证书，公开分发时还可能出现 SmartScreen 提示；这和 Tauri updater 的更新签名是两套不同的签名。

配置快照包含本机路径和 P4 连接信息，发布给其他团队前应替换为自己的初始化配置；如果仓库公开，不要提交当前机器的真实 `release/config/config.json`。

## 计划任务启动

```powershell
release\MHAutoUpdateCompilerNext.exe --run-task MHA_Client_main
release\MHAutoUpdateCompilerNext.exe --run-task MHA_Client_main --dry-run
release\MHAutoUpdateCompilerNext.exe --run-task MHA_Client_main --show-gui
```

也可用工作区 UUID 替代名称；`--data-dir <目录>` 指定隔离的数据目录。计划任务使用 UUID 和显式数据目录。
软件新注册的计划任务会附带 `--show-gui`，自动打开独立任务窗口。执行进程与展示窗口分离：任务返回真实构建结果，展示窗口可以保留，不妨碍下一次计划任务执行。窗口显示完整的定时流程，不改写手动勾选项；停止按钮控制该次任务及其子进程。配置或环境检查失败也会打开错误窗口。`--check-task` 始终只检查环境，不打开任务窗口或执行构建。没有 `--show-gui` 时仍可无窗口执行。
无窗口模式默认读取 exe 同级 `config`，将日志写入同级 `logs`，执行完成后将索引写入 `config\history`。计划任务使用的 `--data-dir` 只决定配置和索引位置，日志仍保存在 exe 同级 `logs`。升级迁移或移动程序后，已有计划任务的显式配置路径也需要更新，可在软件内重新注册；触发时间保持不变。

启动阶段的配置错误、环境检查失败或项目占用也会写入 `scheduled_task_YYYYMMDD.log`。排查计划任务时，可在原有 `--run-task` 参数后追加 `--check-task`：只检查所需路径和工具，不同步代码、不编译、不关闭进程，并输出检查结果。Windows 任务最后结果为 `1` 时，应查看同日日志里的具体错误，而不是反复启动真实构建。

任务执行窗口通过 `config\live-runs` 下的状态快照跟进该次任务，快照只保留最近 1200 条日志，即使归档 JSONL 已达到大小上限，实时窗口仍会继续更新。快照短暂被其他进程占用时自动重试，不中断构建或停止请求。完整文本日志仍写入同级 `logs`。任务完成后保留最后快照，以便窗口继续显示结果。

## 验证边界

交付前验证了配置迁移和保存、命令生成、失败与取消、真实桌面事件和日志、计划任务脚本语法，以及深浅主题和窄屏布局。
2026-09-07 在用户授权下通过原有 Windows 计划任务实际运行 MHA_Client_main，9 个步骤全部完成，Windows 最后结果为 0。实际完成 P4 同步、UE 工程文件生成、1661 个 UE 编译步骤（0 失败）、TS 定义生成，并启动独立 TS Watch；GUI 保留最终结果，任务恢复 Ready。此结果不代表其他工作区也已完成实测，也不把启动 Watch 等同于其持续编译没有源码错误。
实测处理了项目工具占用和 Rider 调试器残留造成的文件锁；另外备份并移走了该目标过期的 Makefile.bin，让 UE 重新生成热更新标识，未修改游戏源码。
带 GUI 的隔离任务已验证成功、取消和启动失败；真实 Windows 计划任务的隔离验证确认了任务结束后窗口可保留、系统任务返回真实退出码。另有快照文件短暂占用的回归测试。以上隔离测试不执行真实 P4、UE 或 TS 命令。
