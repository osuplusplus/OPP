# osu-replay-render

用 wgpu 渲染 osu!standard 回放的 **lib + CLI** 二合一 crate,皮肤实现
osu!lazer 的 **Argon** 默认皮肤。判定与游戏状态来自
[osu-replay-judge](../osu-replay-judge)(逐事件与 lazer 全等的判定模拟器)。
支持两种使用方式:CLI 离屏渲染出视频/PNG;作为库嵌入其他程序做
**实时预览**——离屏读回(`Renderer::render_deferred` + `read_oldest_into`)
跨平台,由宿主把 RGBA 帧送到自己的展示层;Windows 下另有
`SurfaceRenderer` 窗口直渲(`#[cfg(windows)]`,零拷贝 present,高帧率)。
OPP 两者都用:Windows 走 `SurfaceRenderer` 原生直渲(创建失败自动回退
canvas),其他平台走离屏读回 + canvas。

## 用法

```
osu_replay_render <beatmap.osu> [replay.osr] [options]
```

| 选项 | 说明 |
| --- | --- |
| `--autoplay` | **Autoplay mod**：不输入 .osr,由谱面直接生成回放（本地移植 lazer `OsuAutoGenerator`）。判定引擎照常判定生成帧（SS/满血/UR 0）;只负责生成回放,不对 HUD 做任何处理（玩家名显示 lazer 的 autoplay 署名 `osu!`） |
| `--hd` | **强制 Hidden mod 视觉（on）**，覆盖回放原有 mods：物件提前淡出、缩圈隐藏（首个物件保留）。纯视觉覆盖——判定/分数仍按回放真实 mods 计算（HD 本就不影响判定） |
| `--no-hd` | **强制关闭 HD 视觉（off）**，即使回放自带 HD 也以全可见渲染。缺省为 **auto**：跟随回放自身 mods |
| `--out <file.mp4>` | 管道输出到 ffmpeg 编码为 mp4 (h264, crf 18) |
| `--png-dir <dir>` | 输出 PNG 帧序列到目录 |
| `--size <WxH>` | 输出分辨率，默认 1920x1080 |
| `--fps <n>` | 输出帧率 1..480，默认 60（与游戏帧一一对应；其他帧率对光标做插值采样） |
| `--start <ms>` / `--end <ms>` | 渲染回放时间区间（毫秒） |
| `--score classic` | HUD 显示经典分（默认 standardised） |
| `--skin <argon\|argon-pro\|dir>` | 皮肤变体,默认 `argon-pro`(无 GREAT/PERFECT 判定文字、滑条身体透明度 0.92);传**皮肤目录**(解包的 .osk / 游戏 `Skins/<name>`)即启用该皮肤:玩法元素之外,**HUD 的分数/准确率/连击计数器、HP 血条(scorebar-\*,新旧两种样式)与按键计数(inputoverlay-\*)**也换用皮肤自己的贴图(lazer `LegacyScoreCounter`/`LegacyAccuracyCounter`/`LegacyDefaultComboCounter`/`LegacyHealthDisplay`/`LegacyKeyCounterDisplay` 的移植;皮肤缺哪块就回退哪块的 argon 实现,UR 条始终用本渲染器的实现) |
| `--argon-hud` | 有皮肤目录时仍强制 Argon HUD(默认皮肤提供哪块 HUD 元素就用哪块) |
| `--skin-colours` | **强制用皮肤 combo 色**覆盖谱面 `[Colours]`（stable 行为，= lazer 关闭「Beatmap skins」设定）。默认相反：谱面自带 `[Colours]` 时优先谱面色（lazer 默认行为，`LegacyBeatmapSkin` 先应答查找），皮肤 combo 色仅在谱面未配色时生效 |
| `--encoder <auto\|x264\|x265\|nvenc>` | 视频编码器：默认 `auto`（探测 NVENC 可用则用之，否则回退 x264）；NVENC 硬件编码（bgr0 直喂 + p5/hq/vbr/cq，**端到端约比 x264 快 1.7×、比 x265 快 3.2×**）；libx264 / libx265（preset medium + crf） |
| `--quality <n>` | crf（软件）/ cq（nvenc），默认 18 |
| `--hud <on\|off>` | 玩法 HUD 显隐（分数/准确率/连击/血条/UR 条/按键显示/PP 计数器），默认 **on**;`off` 时玩法物件与光标照常渲染。裸 `--hud` 等价 `on`;真值别名 `true/false/1/0`。与 `--autoplay` 互相独立（autoplay 不再隐含关 HUD）。config 键为 JSON 布尔 `"hud"`;库嵌入方用 `HudState.visible` |
| `--no-guides` | 关闭 UR 条的窗口引导线（判定色色轴），默认开启渲染 |
| `--no-pp` | 关闭实时 PP 计数器（lazer 的 legacy HUD 本没有 PP，渲染器默认给所有皮肤额外显示）；库嵌入方用 `HudState.pp_display` |
| `--audio [file]` | 输出混入 BGM（AAC 192k）：带路径用指定文件；不带值自动取谱面 `[General] AudioFilename`（相对谱面目录）。音频位置 = 回放时间 − 偏移；负的起始位置用前置静音（adelay）补齐（复刻 lazer 负时间不播歌的行为）。DT/HT 自动 `atempo` 变速**不变调**（时长随 rate 压缩、音调保持原曲,刻意偏离游戏内变调行为）;NC 保持游戏内 `asetrate` 变速**变调**（nightcore 的升调是其本体,音效仍原速） |
| `--audio-offset <ms>` | BGM 对齐偏移，默认 **0**；需要时手动传入（内部按 lazer 语义 ×rate） |
| `--bgm-volume <0..1>` | BGM 增益,默认 **0.6**(osu! 默认 `VolumeMusic`,`OsuGame.GetFrameworkConfigDefaults` 覆盖 framework 的 1.0) |
| `--hitsounds-volume <0..1>` | 音效总线增益,默认 **0.6**(osu! 默认 `VolumeEffect`)。音效按谱面 authored 电平播放,仅在编码处以 tanh 软拐点代替硬削波 |
| `--master-volume <0..1>` | 混合后主音量,默认 **0.6**(osu! 默认 `VolumeUniversal`)。游戏链路为通道×主音量(0.6×0.6=每总线 0.36),默认下音乐+音效峰约 0.72,基本不削波;要更响的成片可提到 1.0 |
| `--hitsounds` | 导出时离线合成**单独一条音效轨**并混入输出(与 `--audio` BGM 经 `amix` 求和,`normalize=0` 保持原始比例;无 BGM 时音效轨即音轨)。复刻 lazer 游戏音频语义:仅命中判定触发(`ArmedState.Hit`,miss 不发声);音色/音量按 .osu 采样数据解析(时间点 bank/音量、对象 hitSample、滑条 edgeSounds/edgeSets;采样点取 `CONTROL_POINT_LENIENCY` 5ms 语义);圆点=hitnormal+whistle/finish/clap,滑条=头/反复/尾节点音+slidertick+跟踪期 sliderslide/sliderwhistle 循环(跟踪断开即截断);音量下限 5%、声像随物件 X(`PositionalHitsoundsLevel` 0.8)、采样**原速回放**——任何 mod 下都不变调(含 NC),DT/HT/NC 只压缩触发时机;MISS 不触发物件音,连击归零播 `combobreak`(`ComboEffects`:旧 combo>20 或首次中断,`AlwaysPlayFirstComboBreak` 默认开;该样本 ArgonPro 集没有,按查找链取 Argon 集)。音源为 **ArgonPro** 资源集(内嵌):所有 gameplay 查找都命中该集、查找链不再下落——其滑条滑动循环音(sliderslide/sliderwhistle)是空条目=**静音**,即 ArgonPro 不播滑条滑动声;头/尾/反复节点音与 slidertick 为真采样,正常播放 |
| `--bg` | 绘制谱面背景图（`[Events]` 的 `0,0,"..."`，相对谱面目录，PNG/JPEG），全屏铺满 |
| `--bg-opacity <0..1>` | 背景不透明度，默认 **0.3** = 1 − DimLevel（lazer `OsuSetting.DimLevel` 默认 0.7，与游戏内"背景暗度"语义一致） |
| `--results <secs>` | 玩法结束后追加结算界面（lazer `Screens/Ranking`）：**展开状态的 ScorePanel** 静态终帧——顶部头像/用户名条、标题/作者、准确率环（背景环 + 渐变计量表 + D~SS 分级色环 + 达成档位徽章 + 大档位字母）、总分、星数胶囊/模式图标/Mod 徽章、难度名与作者、ACCURACY/COMBO/PP 与判定统计行（GREAT/OK/MEH/MISS 及 L TICK/SLIDER TAIL/BONUS 行），底部 #333 按钮栏。背景为谱面背景图的高斯模糊副本（lazer `ResultsScreen` 的 `BACKGROUND_BLUR` σ=10px@1080p）按 `Gray(0.5)` 压暗铺满，与 lazer 一致；谱面无背景图时为清屏色。不做入场动画（准确率环/计数器直接呈终值）；rank 按 `RankFromScore` 截断 + osu! miss 降级 + HD 银牌计算。默认 **4 秒**；带音频导出时音轨自动以静音补齐到结算屏结束。`--no-results` 关闭。**`--results-only`** 则完全不渲染玩法、只输出结算屏（海报/预览模式，时长同样由 `--results` 控制；单图示例：`--results-only --png-dir out --fps 1 --results 1`） |
| `--avatar <image>` | 结算屏头像图片（jpg/png）：居中裁方 + 预圆角（与占位框同为 20/80 圆角），画在头像框里；不传时沿用玩家首字母占位。等价 config 键 `"avatar"` |
| `--config <file.json>` | JSON 配置文件：键与 CLI 长参数一一对应（snake_case），如 `{"avatar": "a.png", "out": "x.mp4", "results": 5, "results_only": true, "bg": true, "bg_opacity": 0.3, "skin": "dir", "size": "1920x1080", "fps": 60, "hd": "on", "hitsounds": true, "master_volume": 0.8, "ffmpeg_extra": ["-movflags", "+faststart"]}`。config 先应用，**显式 CLI 参数始终覆盖 config**（与出现顺序无关） |
| `--limit <n>` | 最多渲染 n 帧（测试用） |

示例：

```
osu_replay_render map.osu replay.osr --out out.mp4
osu_replay_render map.osu replay.osr --png-dir frames --size 1280x720 --fps 30 --start 10000 --end 20000
osu_replay_render map.osu --autoplay --audio --out preview.mp4   # 谱面预览（无需回放文件）
```

帧时间轴与游戏内一致：60fps 时逐帧对应 lazer 的 FrameStability 游戏帧；
DT/HT 等 rate mod 的回放按真实游戏速度输出。渲染速度约 240fps（1080p
x264,瓶颈在 ffmpeg 编码；渲染管线本身约 0.6ms/帧,见下文「渲染流水线」）。

## 作为库使用（实时预览）

crate 同时提供 `[lib]`,渲染是**任意时刻 t 的纯函数**（`game::snapshot_at`
二分查找 + 光标插值），因此 seek/暂停/逐帧/倍速都是 O(1) 操作,天然适合
实时回放查看器:

```rust
use osu_replay_render::{build_atlas, draw, game, scene, surface};

// 一次性加载:判定引擎跑完整回放,得到逐帧快照时间轴。
let game = game::load("map.osu", "replay.osr")?;
let (atlas, bold, semibold) = build_atlas(None); // 可选背景图

// 方式一(跨平台,推荐):离屏渲染 + 读回 RGBA,宿主送到自己的展示层。
let mut renderer = render::Renderer::new(1280, 720, &atlas);

// 方式二(仅 Windows):窗口直渲,传 Win32 HWND,letterbox 上屏。
// let mut renderer = surface::SurfaceRenderer::new(1280, 720, &atlas, hwnd)?;
// renderer.resize(w, h);

let mut state = scene::SceneState::new(&game, 1280, 720);
let mut list = draw::DrawList::new();
let t = 12345.0; // 任意时刻,毫秒
let snap = game::snapshot_at(&game, t);
let assets = scene::Assets { atlas: &atlas, bold: &bold, semibold: &semibold };
list.clear();
state.build_frame(&game, &assets, &snap, &mut list);
list.finish();
renderer.render_deferred(&list, [0.055, 0.055, 0.075, 1.0]); // 提交本帧
if renderer.pending_len() > 0 {
    let mut bgra = Vec::new();
    renderer.read_oldest_into(&mut bgra); // 取上一帧读回(流水线,GPU 不空转)
    // BGRA(padded_row 对齐)→ RGBA(tight)后交给宿主,如 OPP 的 canvas。
}
```

- `SurfaceRenderer`（`src/surface.rs`）：复用离屏管线渲染到内部纹理,再用
  letterbox blit 上屏 + `present(Fifo)` vsync;支持任意窗口尺寸,与宿主
  WebView UI 共存（OPP 中配合 `WS_EX_TRANSPARENT` 原生子窗口实现点击穿透）。
- 帧间无状态依赖:每帧独立由 t 决定,可乱序/跳跃求值,无需快进模拟。
- `duration = snapshots.last().time`、起点 `snapshots.first().time`,
  seek 时 clamp 到该区间即可。
- 离屏读回路径（`Renderer::render` / `render_deferred` + `read_oldest`
  三缓冲流水线）保持不变,CLI 与嵌入式逐帧导出均可用。

## 已实现（Argon）

- **圆**：四层渐变圆身（外填充/外渐变/内渐变/内填充）、白描边、combo
  数字（Torus Bold）、approach circle、命中动画复刻
  `ArgonMainCirclePiece.updateStateTransforms`：填充层 150ms OutQuint
  隐藏、数字 75ms 消失、外渐变延迟 12.5ms 变白（80ms）后线性淡出
  （150ms）、描边弹性收缩至 0.8×（400ms OutElasticHalf）+ 800ms 颜色
  渐变；色块闪光（FlashPiece 辉光，framework EdgeEffect `Hollow=false`
  数学：内部全亮、外侧二次衰减，半径 `OBJECT_RADIUS×0.6`）150ms
  OutQuint 弹入后 150ms 弹出——**色块远早于圈消失**；整块（描边圈）
  640ms（`fade_out_time×0.8`）OutQuad 淡出，圈最后消失；miss 100ms
  快速淡出。
- **滑条**：蛇形进入/退出（`ProgressAt` 镜像语义）、深色身体 + accent
  描边（20% 带宽）、滑条球（白环 + 渐变填充 + 方向箭头，入场
  200ms OutQuint 弹入；结束时按 `ArgonSliderBall` 源码**叠加 50ms
  OutQuint 额外淡出**（"intentionally pile on an extra FadeOut to make
  it happen much faster"），箭头同时收缩至 0.9×）、follow circle
  （2.4x 展开/释放/结束动画 + tick 脉冲）、tick 分数点、折返箭头
  （白色胶囊 + FontAwesome AngleDoubleRight 双箭头：几何量取自框架
  FontAwesome5 图集（BMFont 字模 74×60 vs 单 chevron 49×81——每个
  chevron 为单字高的 60/81,中心距 ~6.2u,边缘相接不重叠），颜色
  `accentColour.Darken(4)`（组合色 ×0.2 的深色调,非纯黑）、300ms
  脉冲、首折返延迟淡入）、滑条头圆。尾部圆在
  Argon 皮肤中不可见（与 lazer 一致），身体收缩即尾动画。
- **Spinner**（按 `ArgonSpinnerDisc`/`SpinnerRotationTracker` 源码重写）：
  384u 大圆盘、25 个刻度标记（跟随**真实旋转的毫秒级阻尼**
  `Damp(0.99^elapsed_ms)` + 缓慢环境旋转；命中时额外 +180°）、弹出
  两阶段（0.5p-0.75p 到 0.3/0.2，p-1.5p 到 0.8/1.0）、顶部/底部圆弧
  （完成后 0.31→0.50 加粗，40ms 半衰期阻尼）、左右进度弧（静态背景
  完成时瞬时消失，进度弧 40ms 半衰期 + 青色辉光）、中心双环（tracking
  时 80→40 收缩，完成后冻结）、进度辉光填充（完成后每整圈脉冲闪烁）、
  SPM 计数（595ms 窗口，自首次 tracking 起淡入）、bonus 计数弹出
  （打满显示 MAX 2.8× 弹出）。
- **判定**：文字（GREAT/OK/MEH/MISS…，lazer Argon 单词风格、结果色、
  加色混合、miss 下落旋转）、环形爆炸粒子（按结果数量/距离缩放）、
  slider break 小圆点。
- **Follow points**（`ArgonFollowPoint` 逐元素复刻）：间距 32u 的
  双 chevron——前箭头 `GradientVertical(#FC618F→#BB1A41)` 加色混合,
  后箭头为其 `Gray(0.2)` 乘入渐变（每通道 ×0.2,近似不可见的深色
  回声）,中心错开半个 sprite（X=4,Size 8）;chevron 几何按 FA5
  ChevronRight 真实字模（臂与轴 atan(0.5/0.605)≈40°,轴向深度
  0.605×字高,描边 48/512×box）。渐进滑入 + 淡出。
- **光标**：粉渐变环 + 内白环 + 中心点 + 青色辉光，按下弹性放大；
  光标轨迹（加色、(1-age)^4 衰减、300ms 生命）。
- **HD（Hidden mod）**：复刻 `OsuModHidden`——非滑条物件（含滑条头/tick）
  淡入改为 `preempt×0.4`（滑条身体保持默认淡入以对齐 stable），圆/滑条头在
  `[start−preempt+fadeIn, +0.3×preempt]` 线性淡出（打击前 30% preempt 完全
  隐形）；滑条身体自默认淡入结束到 `EndTime` 整段 `Easing.Out` 长淡出，球与
  follow circle 不淡（球悬浮在隐形轨道上，与 lazer 一致）；tick 在
  `min(preempt−150, 1000)ms` 窗口内淡至自身时刻；spinner 在 `EndTime` 之后才
  以 `0.3×preempt` 淡出；approach circle 全部隐藏（仅全图首个非 spinner 物件
  保留，`IncreaseFirstObjectVisibility` 默认开，其缩圈淡入用 HD 调整后的
  `TimeFadeIn×2`）；折返箭头不受影响（Argon 的 repeat `CirclePiece` 为
  `Empty()`）。HD 不改变任何判定（判定端仅用于分数系数 ×1.04），mods 位来自
  回放文件自动启用。
- **HUD**：楔形块、分数/准确率/连击计数器（argon-counter 官方纹理
  数字 + 线框背景 + ink 对齐度量）、数字滚动（250ms）、连击弹出/miss
  变红、血条（judge 判定引擎的 lazer `HealthProcessor` 移植：DR 相关扣
  血表、combo 尾加成、break 段无 drain、满血模拟解 drain rate —— 见
  osu-replay-judge v0.2.0；渲染端 200ms OutQuint 平滑追宽 + 受伤闪红）、**UR 条**（水平置于屏幕底部
  居中，按 lazer `BarHitErrorMeter` 源码移植：刻度为**加色混合**判定
  色竖线（100ms 弹入至 0.6 后 5s 淡出收缩，最多 50 个）、判定色窗口
  引导线色轴（中心 Great 蓝向外 Ok 绿、Meh 黄，最外端渐隐；`--no-guides`
  可关闭，默认渲染）、Great 色中心圆标记、**EMA 均值小箭头**（指数
  移动平均 0.9/0.1，800ms OutQuint 滑动指向）、条上方实时 UR 数值；
  UR 事件集与 `ScoreProcessor.unstable_rate` 完全一致（`has_windows &&
  is_hit`，offset/rate），Welford 增量累计）、**PP 计数器**
  （`ArgonPerformancePointsCounter`：ACC 正下方、整数 + 蓝色 "PP" 标签
  （Torus Bold 12, Blue0）+ 线框背景，250ms 滚动；数据来自
  `rosu-pp` 的 `OsuGradualPerformance` 逐物件推进——judge 判定时间轴
  逐事件折入累计 `OsuScoreState`，每个顶层物件判完推进一次，即
  「每次判定后的实时 PP」，与 lazer 游戏内计数器每 `NewJudgement`
  重算的语义一致；**所有皮肤都显示**（对 lazer 的偏离：legacy 皮肤的
  MainHUD 容器本无 PP 计数器——legacy 布局下它挂在 legacy ACC 行下方）；
  stable 回放走 `lazer(false)` 稳定语义（tick 不计
  acc），lazer 回放计入 slider tick/repeat/tail 命中——rosu-pp 为
  lazer 计算器的移植，数值与原版"大致相同"而非逐位一致）。

## 已实现（自定义皮肤 HUD，`--skin <dir>`）

皮肤目录下,HUD 五件套按 lazer 对应组件逐行移植（坐标为 lazer
1024x768 HUD 空间,`STABLE_MAGIC_SCALE_FACTOR` 1.6 已折入常量;
皮肤缺某块元素时该块回退 argon,`--argon-hud` 可整体强制 Argon）：

- **分数计数器**（`LegacyScoreCounter`）：右上 Anchor TopRight、Scale
  0.96、边距 10,score 字体数字 FixedWidth（'5' 定宽 − ScoreOverlap）,
  按 standardised/classic 补零到 6/8 位;比例滚动（1000·|Δ| ms Out,
  首值瞬时显示不播滚动）。
- **准确率计数器**（`LegacyAccuracyCounter`）：TopRight、Scale
  0.6×0.96、边距 9/17,固定钉在分数行下方（`MainHUD` 容器回调语义）;
  `FormatAccuracy` 文本（向下取整 4 位小数防 89.9999% 显示成 90%）,
  `.`/`%`/`,`/`x` 走 `{prefix}-dot/-percent/-comma/-x` 字形,缺字形跳过
  且不推进 pen（framework `TextBuilder` 语义）。
- **连击计数器**（`LegacyDefaultComboCounter`）：左下 BottomLeft、
  Scale 1.28、边距 10,Combo 字体 `{n}x`；每次 +1 触发**加色大弹出**
  （新值文本 1.56→1 缩放、0.6→0 淡出,300ms 线性）与显示值小脉冲
  （1→1.1→1,50+50ms In/Out）,显示值在 160ms 后步进（
  `big_pop_out_duration − 140` 的调度步进）;断连按比例时长滚动归零
  （差值 ×20ms,线性）并 100ms 淡出。
- **HP 血条**（`LegacyHealthDisplay`）：`scorebar-bg` 贴屏幕左上角,
  `scorebar-colour`（可帧行）按 HP 水平裁切、200ms OutQuint 平滑追宽;
  **新样式**（皮肤提供 `scorebar-marker`）填充/圆盘 marker 按血量着色
  （>0.5 白→0.5 黑→0.2 红的插值曲线,marker ≥0.5 加色混合）,偏移
  (7.5,7.8)×1.6;**旧样式**用 `scorebar-ki/kidanger/kidanger2` 三态
  切换（0.5/0.2 阈值）,偏移 (3,10)×1.6,marker 骑在填充上沿;掉血时
  marker 加色爆闪（120ms Out,血量 ≥0.5 时爆到 2 倍）,回血时 bulge
  （1.2→0.8,150ms）。
- **按键计数**（`LegacyKeyCounterDisplay`）：右缘垂直一列 46x46 盒
  （CentreRight 锚、TopRight 原点、(0,−40)×1.6,间距 1.8）,
  `inputoverlay-background` 横条贴图旋转 90° 缩放 (1.05,1) 挂在列后;
  `inputoverlay-key` 盒子按下压扁到 0.75（160ms Out）,前两键点亮
  #ffde00、第三键 #f8009e;盒内先显示键名,首次按下后永久切换为累计
  按压次数（`scoreentry` 字体,染色 `[Colours] InputOverlayText`
  ?? 黑）;皮肤不带 entry 数字时保留键名（对 lazer 空白行为的小偏离,
  注释已标注）。

## 资产

`assets/`(全部经 `include_bytes!` 内嵌进二进制,**运行时不依赖 CWD**,
可直接被其他程序作为库调用):
- `fonts/Torus-*.otf` — 文字渲染（Torus，与 lazer 一致；ab_glyph 启动时按 24/48/96 三档
  em 栅格化进图集）。
- `counter/argon-counter-*.png` — lazer 官方 HUD 计数器纹理数字
  （来自 [osu-resources](https://github.com/ppy/osu-resources)，MIT/CC-BY-NC 4.0）。
- `cursor/cursortrail.png` — 官方光标轨迹点。
- `cursor/cursor-smoke.png` — 官方烟迹粒子（64×64，来自 osu-resources；
  烟迹渲染实现后直接可用）。
- `spinner/spinner-glow.png` — 官方 spinner 侧弧辉光渐变条（1×107；
  lazer 中由 `ArgonSpinnerProgressArc.ProgressFill` 配合 SpinnerGlow
  shader 做径向采样——当前渲染器用加色 SDF 弧近似该辉光，此纹理留作
  后续实现对应 shader 时使用）。

> Argon/Argon-Pro 皮肤其余元素（圆、描边、辉光、滑条、spinner、光标
> 等）在 lazer 中全部为**程序化矢量绘制**（osu-resources 中除
> `argon-counter-*`、`approachcircle`、`repeat-edge-piece`、
> `cursortrail`、`cursor-smoke`、`spinner-glow` 外无 argon 纹理，
> `ring-glow`/`disc`/`number` 等仅用于 classic/legacy 皮肤，menu-cursor
> 仅菜单用），已在渲染器中以 SDF/图元逐一直接复刻，无可再提取的纹理
> 资产。

## 渲染架构

- wgpu 离屏（无窗口），DX12/Vulkan，4x MSAA；BGRA8 读回后喂给
  ffmpeg rawvideo 管道或写成 PNG;或经 `SurfaceRenderer` letterbox
  blit 到窗口 surface 直渲（present Fifo vsync）。
- 编码：BGRA 管道喂 ffmpeg。`nvenc` 以 bgr0 直喂 NVENC（硬件色彩转换，
  无 CPU swscale）；`x264`/`x265` 走 CPU swscale 转 yuv420p。
- 每帧 CPU 侧构建 `DrawList`：SDF 圆环/圆盘/辉光/胶囊/圆弧、纹理
  四边形、滑条身体带描边条带（MSAA 抗锯齿），按 alpha/加色混合分段
  绘制，绘制顺序复刻 `OsuPlayfield` 层级（spinner → follow points →
  判定爆炸 → 物件(早者在上) → 判定文字 → approach → 光标 → HUD）。
- 游戏状态：判定引擎跑完整回放后输出逐帧快照（光标插值位置、按键、
  slider tracking、spinner 旋转/进度）+ 判定事件时间轴，渲染端据此
  求值任意时刻的视觉状态（`game::snapshot_at` 支持任意输出帧率）。

## 渲染流水线（离屏 CLI 路径）

渲染与编码三级重叠，CPU 与 GPU 互不空等（`main.rs` 主循环 +
`render.rs` 的 `render_deferred`/`read_oldest_into`）：

1. **GPU 回读环形缓冲**（3 个 readback buffer）：每帧 `render_deferred`
   只提交不等待；积压达到深度 2 才 `read_oldest_into` 映射最老的一帧。
   CPU 建下一帧场景时 GPU 手里始终有已排队的渲染工作。
2. **独立写出线程 + 有界通道**（深度 3,天然背压）：帧数据经
   `SyncSender` 发给拥有 ffmpeg stdin 的写出线程,每帧单次
   `write_all`;编码慢时阻塞在 `send` 而不是拖慢渲染。
3. **帧缓冲复用**：写出线程用完的 `Vec<u8>`（1080p 约 8MB）经回传
   通道还回渲染线程,`read_oldest_into` 直接从 GPU 映射区拷入,
   每帧零大分配、单次拷贝。

实测（1080p60, x264 medium crf18, 1800 帧）：流水线化前 ~165fps,
之后 ~245fps,基本贴住 ffmpeg 单独编码的上限（~255fps）;渲染管线
本身（提交+回读）仅 ~0.6ms/帧。输出与流水线化前逐字节一致。
PNG 模式不走流水线（`Renderer::render` 同步路径）。

## Autoplay 谱面预览（`--autoplay`）

`src/autoplay.rs` 是 lazer `OsuAutoGenerator` 的逐行移植
（`osu.Game.Rulesets.Osu/Replays/OsuAutoGenerator.cs`、
`OsuAutoGeneratorBase.cs`、`HasPathWithRepeatsExtensions.cs`）,
按谱面直接生成 `ReplayFrame` 序列喂给判定引擎——判定、HP、combo、
UR、光标轨迹、按键显示全部走既有回放路径,无需 .osr 文件。

保真细节:

- 移动 `Easing.Out`（框架语义 = OutQuad `t(2−t)`）,帧间隔
  `1000/60ms`;反应等待 `preempt − 100ms`（`getReactionTime`）。
- 双键交替:间隔 < 266ms（约快于 225BPM 单双）时递增
  `buttonIndex` 交替左右键;与前一帧按键冲突时强制换手并重写
  后续帧（`addHitObjectClickFrames` 的索引操作全部照抄）。
- 滑条跟随走 span 折返进度（`ProgressAt`/`SpanAt`:奇数 span 反向）,
  非简单 `position_at(j/duration)`。
- 转盘:切线入场（含原版"Y 旋转读已更新 X"的语句顺序怪癖）,
  0.05 rad/ms（≈477 RPM）,入场方向决定旋转方向,外部进入时
  改 `Easing.In` 立即起转。
- 松键帧 = 物件结束 + `KEY_UP_DELAY` 50ms（转盘再 +1ms）;
  0 spins 转盘整物件跳过;时间戳插值保序（`FindInsertionIndex`
  跳过相等时间的语义用 `partition_point` 等价实现）。
- 键位帧在游戏内表示为 `OsuAction.LeftButton/RightButton`,这里
  映射到 `ReplayFrame` 的 `left`/`right` 位。

验证:ReI [Rain]（1111 物件）autoplay 全图 = 满连 1432x、
100.00%、满血,HUD/判定特效与真回放渲染无差异。

## 依赖

- Rust (`cargo build --release`)
- ffmpeg 在 PATH（`--out` 模式需要;库/surface 路径不需要）
- 判定引擎：`osu-replay-judge`（git dependency，master 分支）。本地
  checkout（`../osu-replay-judge`，v0.3.0：HP 处理器双路径 + break 解析）
  经 Cargo.toml 的 `[patch]` 段优先生效——judge 推送后可移除该段
- PP 计算：`rosu-pp`（[Apeuriox fork](https://github.com/Apeuriox/rosu-pp)
  的 `pp-rework-202607` 分支，lazer 难度/表现计算器的 Rust 移植），同样
  经 `[patch]` 指向本地 `../rosu-pp` checkout
- 作为库嵌入时:宿主窗口需提供原生窗口句柄(当前为 Win32 HWND;
  `raw-window-handle` 0.6)

## 开发

- 源码结构：`lib.rs`（库入口:图集构建/资产解码,全部贴图内嵌）、
  `main.rs`（CLI/编码管线）、`surface.rs`（窗口 surface 直渲 +
  letterbox blit,实时预览用）、`game.rs`（judge 输出 → 渲染
  视图 + UR/HP 时间轴）、`autoplay.rs`（lazer `OsuAutoGenerator`
  的逐行移植,`--autoplay` 谱面预览）、`scene.rs`（Argon 皮肤全部
  视觉逻辑）、`hud.rs`（计数器/血条/UR 条）、`draw.rs`（SDF
  图元/缓动/字体/图集）、`render.rs`（wgpu 离屏渲染器 + WGSL
  shader + readback 流水线）。
- 所有动画时序均以 osu!lazer 源码（`osu.Game.Rulesets.Osu/Skinning/Argon/`
  等）与 osu-framework（`Interpolation.Damp` 系列为**毫秒指数**语义）
  为准；调参前先对照源文件注释。
- 实际嵌入示例：[OPP](../OPP) 的 `src-tauri/src/live_render.rs`
  （Tauri 原生子窗口 + 播放线程 + seek/play/pause 命令,前端配合
  DOM 位置上报与可拖动进度条）。

## 许可

[MIT](LICENSE)
