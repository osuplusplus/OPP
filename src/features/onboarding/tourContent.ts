export const CURRENT_ONBOARDING_VERSION = 1;

export function needsOnboarding(version: number | null | undefined) {
  return (version ?? 0) < CURRENT_ONBOARDING_VERSION;
}

export interface OnboardingStep {
  title: string;
  description: string;
  target?: string;
  targetText?: string;
  targetTextSelector?: string;
  expandSelector?: string;
  expandTarget?: boolean;
  example?: string;
}

export const onboardingSteps: OnboardingStep[] = [
  {
    title: "欢迎使用 OPP",
    description: "接下来会用几分钟带你认识 OPP 的主要功能。你可以随时返回上一步，也可以跳过引导。",
  },
  {
    title: "在线谱面与收藏夹",
    description: "搜索和试听在线谱面、查看详情并批量下载。下载失败时会自动回退到其他镜像；喜欢的谱面还可以统一加入收藏夹管理。",
    target: '[data-onboarding="online-and-collections"]',
  },
  {
    title: "相似谱面",
    description: "以指定谱面、最近成绩或 BP 为种子，从本地索引中寻找相似谱面。standard 使用 Analyzer v4，Mania 使用独立 Analyzer v1 数据集。",
    target: '[data-onboarding="similar-beatmaps"]',
  },
  {
    title: "技能分析",
    description: "分析最佳成绩中的八项能力，识别优势与短板，并把成绩直接带入相似谱面和练习生成器。",
    target: '[data-onboarding="skill-analysis"]',
  },
  {
    title: "谱面练习生成器",
    description: "调整速度、AR、OD、CS、HP、BPM 或时间范围，生成独立的训练副本；原谱面不会被修改。",
    target: '[data-onboarding="trainer"]',
  },
  {
    title: "数据中心",
    description: "集中查看个人资料、排名轨迹、统计数据、BP、Pinned 成绩与详细档案。这里的内容会跟随当前游戏模式变化。",
    target: '[data-onboarding="data-center"]',
  },
  {
    title: "本地资源",
    description: "扫描和管理本地谱面、皮肤、截图与回放，并在应用内预览资源。使用前请先在设置中确认 Stable 或 Lazer 的游戏目录。",
    target: '[data-onboarding="local-resources"]',
  },
  {
    title: "回放渲染",
    description: "将符合条件的 osu!standard 回放提交给渲染服务，跟踪任务进度，并把完成的视频导出到本地。",
    target: '[data-onboarding="replay-render"]',
  },
  {
    title: "工具集合",
    description: "这里包含手速测试、.osz/.osk 默认打开端、显示器伽马调整、Malody 谱面转换，以及 tosu 直播和 OBS 集成。",
    target: '[data-onboarding="tools"]',
  },
  {
    title: "模式与客户端",
    description: "打开“设置 → 常规”切换 osu!、Taiko、Catch 或 Mania，以及 Stable／Lazer 客户端。选择会立即应用并自动保存。",
    target: '[data-onboarding="settings"]',
  },
  {
    title: "启动 osu!",
    description: "这里集中显示游戏运行状态。点击主按钮启动设置中选择的客户端，右侧箭头可启动另一客户端；游戏结束后可查看本次数据变化。",
    target: '[data-onboarding="start-game"]',
  },
  {
    title: "设置",
    description: "在设置中管理主题、试听音量、游戏目录、下载位置与镜像、缓存上限和高级功能。",
    target: '[data-onboarding="settings"]',
  },
  {
    title: "准备就绪",
    description: "引导到这里就完成了。以后想再看一遍，可以打开“设置”，在“关于”区域点击“重新查看新手引导”。",
  },
];
