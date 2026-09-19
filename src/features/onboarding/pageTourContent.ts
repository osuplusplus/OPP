import type { OnboardingStep } from "./tourContent";

export const CURRENT_PAGE_ONBOARDING_VERSION = 2;
const TRAINER_EMPTY_STATE_ONBOARDING_VERSION = 3;

export interface PageGuide {
  id: string;
  title: string;
  version: number;
  steps: OnboardingStep[];
}

interface ModuleGuide {
  title: string;
  purpose: string;
  example: string;
  target?: string;
  targetText?: string;
  targetTextSelector?: string;
  expandSelector?: string;
  expandTarget?: boolean;
}

interface PageGuideCopy {
  id: string;
  title: string;
  version?: number;
  summary: string;
  modules: ModuleGuide[];
}

const copies: Record<string, PageGuideCopy> = {
  "/online/beatmaps": {
    id: "online-beatmaps",
    title: "在线谱面",
    version: 4,
    summary: "搜索与筛选后，在右侧挑选谱面，左侧查看、试听或下载；下载清单可跨搜索累积。",
    modules: [
      { title: "搜索想找的内容", purpose: "输入曲名、艺术家、谱师或标签，按回车或点击搜索。输入建议仅供补全，不会自动打开谱面。", example: "输入 Camellia 后按回车；留空则浏览近期 Ranked。", target: '[data-page-guide-online-search="true"]' },
      { title: "按需展开筛选", purpose: "点击筛选按钮打开面板，调整模式、状态、星级及更多条件后应用。关闭时舍弃草稿；已生效的条件显示为可移除标签。", example: "选电子流派和 4–6 星后应用，再点击条件标签移除某项筛选。", target: '[data-page-guide-online-filter-trigger="true"]' },
      { title: "连续挑选谱面", purpose: "右侧结果独立滚动并继续加载。结果常驻显示难度图标，悬停可查看详细难度；点击更新舞台，随机一首会从当前已加载的筛选结果中选择。", example: "悬停检查各难度，或随机选择一首，再在舞台试听和打开完整详情。", target: '[data-page-guide-online-results="true"]' },
      { title: "跨搜索累积清单", purpose: "多选谱面后加入下载清单，也可通过批量加入收集当前筛选与排序的前 50／100／250／500 首。重复谱面会去重。", example: "选几首加入清单，再换一个关键词继续挑选。", target: '[data-page-guide-online-results="true"]' },
      { title: "打开清单开始下载", purpose: "顶栏下载清单显示数量与任务进度。点击检查条目后开始下载，关闭抽屉或切换页面不会中断下载。目录与下载源可在更多选项中调整。", example: "下载期间继续加入下一批；失败和取消后的未完成项仍保留在清单。", target: '[data-page-guide-online-download="true"]' },
    ],
  },
  "/collections": {
    id: "collections",
    title: "谱面收藏夹",
    summary: "这个页面统一管理 Stable 收藏夹、Lazer 只读收藏和 OPP 分享图包。",
    modules: [
      { title: "收藏夹内容", purpose: "左侧每个收藏夹模块展示谱面条目、解析状态和待写回状态，并提供导出、补全下载和删除操作。", example: "在某个收藏夹中移除一个难度，确认出现“待写回”后，再使用顶部“写回游戏”。", target: '[data-page-guide-content="true"]' },
      { title: "新建收藏夹", purpose: "创建一个新的 OPP/Stable 可写收藏夹，之后可从在线、本地或相似谱面页添加难度。", example: "输入“本周练习”，点击创建，再去本地谱面页把目标难度加入其中。", targetText: "新建收藏夹" },
      { title: "导入分享码", purpose: "解析其他用户提供的 OPPC2 分享码，预览后导入为新的收藏夹。", example: "粘贴以 OPPC2. 开头的分享码，点击解析，核对谱面数量后确认导入。", targetText: "导入分享码" },
      { title: "游戏来源", purpose: "说明 Stable/Lazer 收藏数据是否可用、是否只读，帮助判断修改能否写回。", example: "看到 Stable 可写、Lazer 只读时，只对 Stable 收藏夹执行删除或写回。", targetText: "游戏来源" },
    ],
  },
  "/online/similar": {
    id: "similar-beatmaps",
    title: "相似谱面",
    summary: "这个页面按全局模式使用独立索引：osu!standard 使用 Analyzer v4，osu!mania 使用 Analyzer v1，并可从谱面、最近成绩或 BP 生成相似结果。",
    modules: [
      { title: "查询来源", purpose: "选择单个 Beatmap ID、最近成绩或 BP 推荐，并设置返回数量。查询内容只在本机索引中计算。", example: "输入 Beatmap ID 1234567，保留默认权重，点击查找相似谱面。", target: '[data-page-guide-content="true"]' },
      { title: "高级参数", purpose: "osu!standard 高级模式可调整六组固定权重；Mania 先按 Analyzer 分类，再支持 NM、DT、HT 单池或混池。", example: "standard 想强调读图相似度时，展开高级参数并提高 Reading 权重，再重新查询。", targetText: "高级参数", targetTextSelector: "button", expandTarget: true },
      { title: "候选谱面筛选", purpose: "standard 的浮动筛选面板限制 BPM、长度、AR、OD、CS 和星数；Mania 结果固定使用同键数 Ranked 分位与键型特征。", example: "standard 推荐结果过宽时，打开筛选并设置 180–220 BPM、5–7 星。", target: ".opp-candidate-filter-panel", expandSelector: '[aria-label="打开候选谱面筛选"]' },
      { title: "相似结果与对比", purpose: "结果区展示距离、特征对比和推荐来源，可试听、快捷下载或跳转在线谱面详情。", example: "先查看距离最小的结果，再对比雷达图，试听确认风格后下载。", target: '[data-page-guide-content="true"]' },
    ],
  },
  "/trainer": {
    id: "trainer",
    title: "谱面练习生成器",
    summary: "这个页面把从本地谱面导入的难度生成独立训练副本，不会改动原谱面。",
    modules: [
      { title: "源谱面信息", purpose: "顶部模块确认当前难度、BPM、基础参数和物件数，避免对错误的难度生成副本。", example: "确认曲名与难度名称正确，并记录原始 BPM 和 AR/OD。", target: '[data-page-guide-content="true"]' },
      { title: "速度与难度", purpose: "Rate 同步改变音频速度和谱面时间轴，AR、OD、CS、HP 可以独立覆盖。", example: "把 Rate 设置为 1.10、AR 设置为 9.5，其余参数保留原值。", targetText: "速度与难度" },
      { title: "区间训练", purpose: "按 BPM 或起止秒数只保留目标物件，用于拆分长图或练习特定速度段。", example: "设置最低 BPM 190、最高 BPM 220，并截取 60–120 秒。", targetText: "区间训练" },
      { title: "生成训练谱面", purpose: "把当前参数写成 Songs 目录中的新副本；原谱面文件保持不变。", example: "检查参数后点击“一键生成到 Songs”，再在 osu! 中刷新谱面列表。", targetText: "生成训练谱面" },
    ],
  },
  "/data": {
    id: "data-center",
    title: "数据中心",
    summary: "这个页面把当前账户和所选游戏模式的官方资料、成绩与档案放在同一条纵向工作流中。",
    modules: [
      { title: "概览与统计", purpose: "展示 PP、排名、准确率、活跃度和长期趋势，用于快速判断账号当前状态。", example: "切换到 Mania 模式，对比全局排名和近期开图活跃度。", target: "#overview" },
      { title: "BP 1–100", purpose: "列出最高价值的最佳成绩，并支持按曲名、Mapper、难度或 Mod 搜索。", example: "搜索 HDHR，查看该 Mod 组合在 BP 前 100 中的分布。", target: "#scores" },
      { title: "BP 101–200 与 Pinned", purpose: "补充中段 BP，并单独展示个人主页中手动置顶的成绩。", example: "在 Pinned 区检查展示成绩，再与 BP 排名位置对照。", target: "#pinned" },
      { title: "详细档案", purpose: "按身份、社交、徽章、成就和历史用户名等主题浏览完整资料。", example: "切换档案标签查看历史用户名和徽章，不影响当前游戏模式。", target: "#profile" },
    ],
  },
  "/local/maps": {
    id: "local-maps",
    title: "本地谱面",
    version: 3,
    summary: "搜索选中一个谱面集，在背景主舞台上切换难度、试听音乐并使用常用功能。每次进入会随机展示一首。",
    modules: [
      { title: "用搜索找到下一首", purpose: "Ctrl+K 聚焦搜索，输入曲名、谱师、标签或 ID，上下键挑选候选，Enter 确认后展开谱面。筛选可限定星数、BPM、长度等范围。", example: "搜索一位谱师，再按 ↓ 和 Enter 选中谱面集。", target: '.local-set-search' },
      { title: "切换难度与操作", purpose: "底部显示当前游戏模式的全部难度，难度仅显示名称和星级。选择难度后，封面上的参数与底部功能入口随之更新。", example: "选择 Insane，点击“查找相似”或“导入 Trainer”。", target: '.local-stage-console' },
      { title: "本地试听与完整详情", purpose: "点击底部播放图标播放本地音乐，悬停或聚焦播放按钮可调整进度；完整详情保留 strain 曲线与结构数据。", example: "先试听曲目，再打开完整详情查看难度分布。", target: '.local-stage-actions' },
      { title: "管理谱库", purpose: "在谱库管理中更换目录、增量刷新或强制重建索引。扫描进度与异常会直接提示。", example: "安装新谱面后等待自动索引，或在谱库管理中点击增量刷新。", target: '.local-library-trigger' },
    ],
  },
  "/local/skins": {
    id: "local-skins",
    title: "本地皮肤",
    summary: "这个页面浏览 Skin 配置、图片和音效资源，并支持安全替换资源。",
    modules: [
      { title: "数据源设置", purpose: "确认当前客户端与 Skin 目录是否有效，并在目录变化后重新扫描。", example: "切换到 Stable，展开数据源设置并确认 Skins 路径可用。", target: '[aria-label="数据源设置"]', expandTarget: true },
      { title: "Skin 列表与预览", purpose: "选择一个 Skin 后预览封面、游玩元素和可播放音效。", example: "选择常用 Skin，打开光标图片并试听 hitnormal 音效。", target: '[data-page-guide-content="true"]' },
      { title: "Skin 配置段", purpose: "skin.ini 按段折叠展示；展开后可以检查键值、颜色和版本差异。", example: "展开 [General]，检查 Name、Author 和 Version；引导不会自动替换任何文件。", targetText: "[General]", targetTextSelector: "summary", expandTarget: true },
    ],
  },
  "/local/media": {
    id: "local-media",
    title: "截图与回放",
    summary: "这个页面集中浏览当前客户端的截图和回放，并提供预览、复制与定位操作。",
    modules: [
      { title: "媒体类型", purpose: "在截图和回放之间切换，列表会按当前类型和搜索条件更新。", example: "先选择“截图”，查找最近文件；再切到“回放”读取回放信息。", target: '[aria-label="媒体类型"]' },
      { title: "本地媒体列表", purpose: "显示文件名、时间和基础信息，点击条目后在右侧打开详情。", example: "搜索歌曲名，选择匹配的回放文件查看详情。", targetText: "本地媒体" },
      { title: "预览与文件操作", purpose: "截图可预览和复制；回放可读取元数据、复制路径或在资源管理器中定位。", example: "选中截图后复制到剪贴板；选中回放后点击“在资源管理器中显示”。", target: '[data-page-guide-content="true"]' },
    ],
  },
  "/local/media/render": {
    id: "replay-render",
    title: "回放渲染",
    summary: "这个页面可调用本机 Danser 导出视频，也可将兼容回放提交给 o!rdr；引导不会启动真实渲染。",
    modules: [
      { title: "选择渲染方式", purpose: "在本地 Danser 与在线 o!rdr 两个子页面之间切换，未提交的表单不会丢失。", example: "优先使用本地 Danser；没有本地环境时切换到在线 o!rdr。", target: '[aria-label="渲染方式"]' },
      { title: "本地运行环境", purpose: "检查 danser-cli 与 FFmpeg 是否就绪，也可以手动选择便携版程序。", example: "状态显示“已就绪”后再选择回放。", targetText: "本地运行环境" },
      { title: "回放与输出", purpose: "选择本地 osu!standard 回放并设置统一视频导出目录。", example: "选择最近回放，确认已匹配谱面并检查输出位置。", targetText: "回放与输出" },
      { title: "常用与高级参数", purpose: "通过表单设置 Danser 配置档、皮肤、速度、时间段、Mod 与 sPatch。", example: "保留默认配置档，只按需要调整速度或皮肤。", targetText: "常用参数" },
      { title: "本地渲染队列", purpose: "任务会顺序执行，并显示进度、取消操作和最终视频位置。", example: "加入队列后等待完成，再点击“定位视频”。", targetText: "本地渲染队列" },
    ],
  },
  "/tosu": {
    id: "tosu",
    title: "tosu 直播集成",
    summary: "这个页面管理 tosu 服务、自动启动、OBS WebSocket 和实时歌词浏览器源。",
    modules: [
      { title: "tosu 服务", purpose: "选择 tosu.exe、启动或停止由 OPP 管理的服务，并显示 API 是否可达。", example: "先选择 tosu.exe，点击启动，等待顶部状态变为“API 已连接”。", targetText: "tosu 服务" },
      { title: "直播自动化", purpose: "控制检测到 osu! 或 OBS 时是否自动启动 tosu，以及是否显示确认提示。", example: "开启“检测到 OBS 后启动 tosu”，保留启动确认提示。", targetText: "直播自动化" },
      { title: "OBS WebSocket", purpose: "保存 OBS 地址和密码、读取场景，并刷新所选场景中的浏览器源。", example: "填写 ws://127.0.0.1:4455，保存并测试，读取场景后选择直播场景。", targetText: "OBS WebSocket" },
      { title: "OBS 实时歌词", purpose: "管理歌词代理并提供可添加到 OBS 的本地浏览器源地址。", example: "选择 tosu-proxy.exe，启动后复制歌词地址到 OBS 浏览器源。", targetText: "OBS 实时歌词" },
    ],
  },
  "/tools": {
    id: "tools",
    title: "工具集合",
    summary: "这个页面包含彼此独立的 osu! 辅助工具；引导只说明用法，不会自动修改系统设置。",
    modules: [
      { title: "手速测试", purpose: "在指定时长内统计有效按键次数、KPS，并换算近似 BPM。", example: "选择 10 秒，点击开始后交替按两个键，结束时查看 KPS 与 BPM。", targetText: "手速测试" },
      { title: "文件默认打开端", purpose: "设置 Windows 双击 .osz 和 .osk 时交给 Stable 还是 Lazer。", example: "把 .osz 设置为 Lazer、.osk 设置为 Stable，然后分别保存。", targetText: "文件默认打开端" },
      { title: "显示器伽马", purpose: "实时调整主显示器中间调，1.00 为系统默认值，可一键复原。", example: "小幅拖到 1.10 检查画面，测试后点击“复原”回到 1.00。", targetText: "显示器伽马" },
      { title: "Malody 谱面转换", purpose: "把一个或多个 .mcz 转换为同目录下的 .osz，同名输出会安全跳过。", example: "选择两个 .mcz 文件，转换后查看每项完成、跳过或失败状态。", targetText: "Malody 谱面转换" },
    ],
  },
  "/settings": {
    id: "settings",
    title: "设置",
    summary: "这个页面管理账户、外观、默认模式、本地目录、下载和缓存等全局偏好。",
    modules: [
      { title: "账户", purpose: "查看当前连接账户，重新完成 OAuth 认证，或退出并清理登录状态。", example: "授权失效时点击“重新认证”，在浏览器完成授权后返回 OPP。", targetText: "账户" },
      { title: "主题", purpose: "切换浅色/深色外观和主题色，修改会立即应用到整个软件。", example: "开启浅色主题并选择绿色，确认侧栏和按钮颜色同步变化。", targetText: "主题" },
      { title: "模式与客户端", purpose: "在常规设置中切换游戏模式和 Stable／Lazer 客户端，立即应用并自动保存。", example: "选择 Mania 与 Lazer，左下角启动区会同步显示当前选择。", targetText: "常规" },
      { title: "试听", purpose: "控制在线谱面和相似谱面的音频预览音量。", example: "先调到 40%，返回在线谱面试听，再按需要微调。", targetText: "试听" },
      { title: "游戏目录", purpose: "分别管理 Stable 和 Lazer 目录，是本地谱面、Skin、媒体与工具功能的数据基础。", example: "点击自动检测；若失败，手动选择包含 Songs 或对应数据文件的目录。", targetText: "游戏目录" },
      { title: "工具与缓存", purpose: "开启相似谱面高级设置、清理资料缓存并限制本地缩略图占用。", example: "开启高级设置，把缩略图缓存上限设为 512 MB。", targetText: "工具与缓存" },
      { title: "谱面下载", purpose: "选择默认镜像、保存位置，以及下载完成后是否自动交给 osu! 导入。", example: "优先选择小夜，设置 Downloads 文件夹，并开启下载后自动打开。", targetText: "谱面下载" },
      { title: "关于与引导", purpose: "查看版本和项目仓库，并可重新启动软件总体引导。页面标题旁问号只重播设置页引导。", example: "点击“重新查看新手引导”复习总体功能；关闭后仍可用问号回到本页说明。", targetText: "关于" },
    ],
  },
};

function stepsFor(copy: PageGuideCopy): OnboardingStep[] {
  return [
    {
      title: `认识“${copy.title}”`,
      description: copy.summary,
      target: '[data-page-guide-title="true"]',
    },
    ...copy.modules.map((module) => ({
      title: module.title,
      description: module.purpose,
      example: module.example,
      target: module.target,
      targetText: module.targetText,
      targetTextSelector: module.targetTextSelector,
      expandSelector: module.expandSelector,
      expandTarget: module.expandTarget,
    })),
    {
      title: "本页引导完成",
      description: `你已经了解“${copy.title}”的主要模块。以后可以点击页面标题旁的问号，随时重新查看本页引导。`,
    },
  ];
}

export function getPageGuide(pathname: string): PageGuide | null {
  if (pathname === "/trainer" && document.querySelector('[data-page-guide-content="true"] .min-h-64')) {
    return {
      id: "trainer",
      title: "谱面练习生成器",
      version: TRAINER_EMPTY_STATE_ONBOARDING_VERSION,
      steps: [
        {
          title: "先导入一张本地谱面",
          description: "生成器需要一张本地难度作为源文件。当前还没有导入内容，因此这里会显示开始入口。",
          example: "点击“前往本地谱面”，展开一张谱面的难度列表，再选择“导入 Trainer”。",
          target: '[data-page-guide-content="true"] .min-h-64',
        },
        {
          title: "导入后可以做什么",
          description: "导入后可调整速度、AR、OD、CS、HP、BPM 和时间区间，并生成一个不会修改原谱面的训练副本。",
          target: '[data-page-guide-title="true"]',
        },
      ],
    };
  }
  const copy = copies[pathname];
  return copy
    ? { id: copy.id, title: copy.title, version: copy.version ?? CURRENT_PAGE_ONBOARDING_VERSION, steps: stepsFor(copy) }
    : null;
}

export function needsPageOnboarding(storedVersion: number | undefined, guide: PageGuide) {
  return (storedVersion ?? 0) < guide.version;
}
