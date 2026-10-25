import { useEffect, useState } from "react";
import { ExternalLink, Keyboard, Layers } from "lucide-react";
import { Button, Card, SectionTitle, Toggle } from "../../../shared/components/ui";
import type { AppSettings, OverlayMpStatus } from "../../../shared/types/osu";
import { desktopApi } from "../../../shared/lib/tauri";
import { errorMessage } from "../../../shared/lib/format";

type Corner = NonNullable<AppSettings["overlay_corner"]>;

const corners: Array<[Corner, string]> = [
  ["TopRight", "右上角"],
  ["TopLeft", "左上角"],
  ["BottomRight", "右下角"],
  ["BottomLeft", "左下角"],
];

const modifierKeys = new Set(["Control", "Shift", "Alt", "Meta"]);

const ircStateLabels: Record<string, string> = {
  disconnected: "未连接",
  connecting: "连接中…",
  connected: "已连接",
  error: "连接失败",
};

const translationProviders: Array<{ value: string; label: string; hint: string }> = [
  {
    value: "google_free",
    label: "Google 免费端点（零配置）",
    hint: "无需注册；国内需代理（自动检测本机 Clash 等代理），常规文本可用但俚语/梗较弱。",
  },
  {
    value: "tencent",
    label: "腾讯云机器翻译（国内推荐）",
    hint: "每月 500 万字符免费额度，超出部分默认不开后付费不会扣费；需腾讯云实名账号。",
  },
  {
    value: "baidu",
    label: "百度翻译（轻注册）",
    hint: "标准版注册即享 5 万字符/月，个人认证后 100 万字符/月；需百度翻译开放平台账号。",
  },
  {
    value: "openai",
    label: "LLM · OpenAI 兼容（梗翻译最佳）",
    hint: "内置 osu! 玩家黑话对照，俚语/meme 翻译质量最好；GLM-4-Flash 完全免费，亦兼容 DeepSeek/OpenAI/Ollama。",
  },
  { value: "off", label: "关闭翻译", hint: "聊天仅显示原文。" },
];

const targetLanguages: Array<[string, string]> = [
  ["zh", "中文"],
  ["en", "英语"],
  ["ja", "日语"],
  ["ko", "韩语"],
  ["ru", "俄语"],
  ["es", "西班牙语"],
  ["pt", "葡萄牙语"],
  ["de", "德语"],
  ["fr", "法语"],
  ["it", "意大利语"],
  ["tr", "土耳其语"],
  ["th", "泰语"],
  ["vi", "越南语"],
  ["id", "印尼语"],
  ["ar", "阿拉伯语"],
  ["pl", "波兰语"],
];

const openaiPresets: Array<{ label: string; base: string; model: string }> = [
  { label: "GLM-4-Flash（免费）", base: "https://open.bigmodel.cn/api/paas/v4", model: "glm-4-flash" },
  { label: "DeepSeek", base: "https://api.deepseek.com", model: "deepseek-chat" },
  { label: "OpenAI", base: "https://api.openai.com/v1", model: "gpt-4o-mini" },
  { label: "Ollama 本地", base: "http://127.0.0.1:11434/v1", model: "qwen2.5:7b" },
];

interface OverlayPanelProps {
  settings: AppSettings;
  save: (settings: AppSettings) => Promise<void>;
  busy: boolean;
}

export function OverlayPanel({ settings, save, busy }: OverlayPanelProps) {
  const [hotkeyDraft, setHotkeyDraft] = useState(
    settings.overlay_hotkey_toggle ?? "Control+Shift+O",
  );
  const [recording, setRecording] = useState(false);
  const [hotkeyBusy, setHotkeyBusy] = useState(false);
  const [hotkeyError, setHotkeyError] = useState<string | null>(null);
  const [hotkeyDirty, setHotkeyDirty] = useState(false);
  const [status, setStatus] = useState<OverlayMpStatus | null>(null);
  const [ircUsername, setIrcUsername] = useState(settings.overlay_irc_username ?? "");
  const [ircToken, setIrcToken] = useState("");
  const [ircBusy, setIrcBusy] = useState(false);
  const [ircMessage, setIrcMessage] = useState<string | null>(null);
  const [ingameBusy, setIngameBusy] = useState(false);
  const [credentialPrimary, setCredentialPrimary] = useState("");
  const [credentialSecondary, setCredentialSecondary] = useState("");
  const [credentialBusy, setCredentialBusy] = useState(false);
  const [credentialMessage, setCredentialMessage] = useState<string | null>(null);
  const [testBusy, setTestBusy] = useState(false);
  const [testMessage, setTestMessage] = useState<string | null>(null);
  const [apiBase, setApiBase] = useState(settings.translation_api_base_url ?? "");
  const [apiModel, setApiModel] = useState(settings.translation_api_model ?? "");

  const refreshStatus = () => {
    void desktopApi
      .overlayGetMpStatus()
      .then(setStatus)
      .catch(() => {});
  };

  useEffect(() => {
    refreshStatus();
    const timer = setInterval(refreshStatus, 5000);
    return () => clearInterval(timer);
  }, []);

  useEffect(() => {
    if (!recording) return;
    const onKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      if (event.key === "Escape") {
        setRecording(false);
        return;
      }
      if (modifierKeys.has(event.key)) return;
      const parts: string[] = [];
      if (event.ctrlKey) parts.push("Control");
      if (event.shiftKey) parts.push("Shift");
      if (event.altKey) parts.push("Alt");
      if (event.metaKey) parts.push("Meta");
      // 仅录制字母/数字主键，其余（方向键等）请手动输入。
      if (event.key.length !== 1) return;
      const combination = [...parts, event.key.toUpperCase()].join("+");
      setHotkeyDraft(combination);
      setHotkeyDirty(true);
      setRecording(false);
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [recording]);

  const saveHotkey = async () => {
    setHotkeyBusy(true);
    setHotkeyError(null);
    try {
      // 先试注册校验（自动恢复原注册），失败则不落盘。
      await desktopApi.overlayTestHotkey(hotkeyDraft.trim());
      await save({ ...settings, overlay_hotkey_toggle: hotkeyDraft.trim() });
      setHotkeyDirty(false);
    } catch (error) {
      setHotkeyError(errorMessage(error));
    } finally {
      setHotkeyBusy(false);
    }
  };

  const saveIrcCredentials = async () => {
    setIrcBusy(true);
    setIrcMessage(null);
    try {
      const username = ircUsername.trim() || null;
      await desktopApi.overlaySetIrcCredentials(username, ircToken.trim());
      setIrcToken("");
      // 用户名同时写入设置（令牌只进系统密钥环）。
      await save({ ...settings, overlay_irc_username: username });
      setIrcMessage("已保存。游戏运行时会立即尝试连接。");
      refreshStatus();
    } catch (error) {
      setIrcMessage(errorMessage(error));
    } finally {
      setIrcBusy(false);
    }
  };

  const reinstallIngame = async () => {
    setIngameBusy(true);
    try {
      await desktopApi.overlayInstallIngame();
      refreshStatus();
    } finally {
      setIngameBusy(false);
    }
  };

  const translationProvider = settings.translation_provider ?? "google_free";
  const needsCredential =
    translationProvider === "tencent" || translationProvider === "baidu" || translationProvider === "openai";

  const saveTranslationCredential = async () => {
    setCredentialBusy(true);
    setCredentialMessage(null);
    try {
      const primary = credentialPrimary.trim();
      const secondary = credentialSecondary.trim();
      let credential: string | null = null;
      if (translationProvider === "tencent" || translationProvider === "baidu") {
        // 两段式凭据（SecretId/SecretKey、APP ID/密钥）必须成对填写。
        if (primary && !secondary) {
          setCredentialMessage("ID 与密钥需成对填写。");
          return;
        }
        credential = primary && secondary ? `${primary}|${secondary}` : null;
      } else if (translationProvider === "openai") {
        credential = primary || null;
      }
      await desktopApi.translationSetCredential(credential);
      setCredentialPrimary("");
      setCredentialSecondary("");
      setCredentialMessage(credential ? "凭据已保存（仅存系统密钥环）。" : "已清除保存的凭据。");
    } catch (error) {
      setCredentialMessage(errorMessage(error));
    } finally {
      setCredentialBusy(false);
    }
  };

  const runTranslationTest = async () => {
    setTestBusy(true);
    setTestMessage(null);
    try {
      const result = await desktopApi.translationTest();
      setTestMessage(`测试通过：${result.translated_text}`);
    } catch (error) {
      setTestMessage(`测试失败：${errorMessage(error)}`);
    } finally {
      setTestBusy(false);
    }
  };

  const ircLabel = status
    ? `${ircStateLabels[status.irc.state] ?? status.irc.state}${
        status.irc.channels.length ? ` · ${status.irc.channels.join(" ")}` : ""
      }`
    : "读取中…";

  return (
    <div className="space-y-5">
      <div>
        <h2 className="text-2xl font-bold text-white">游戏覆盖</h2>
        <p className="mt-1 text-sm text-slate-400">
          Steam 式游戏内覆盖：平时完全隐藏，按全局热键随时唤出 Direct 谱面面板与多人聊天；
          可选常驻 HUD 状态卡与 tosu 游戏内渲染（独占全屏可见）。
        </p>
      </div>

      <Card className="p-6">
        <SectionTitle
          title="启用与显示"
          description="覆盖层平时不显示，按热键唤出可交互的 Direct 面板，再按热键或 Esc 收起。"
        />
        <div className="mt-5">
          <Toggle
            checked={settings.overlay_enabled ?? false}
            description="开启后注册全局热键并创建覆盖层窗口：多人补缺在后台常驻，游戏画面默认不被遮挡。"
            label="启用游戏内覆盖"
            onChange={(value: boolean) => void save({ ...settings, overlay_enabled: value })}
          />
        </div>
        <div className="mt-3">
          <Toggle
            checked={settings.overlay_start_tosu_with_game !== false}
            description="游戏启动时若 tosu 未运行则自动拉起（仅 OPP 托管启动时生效，并自动写入 tosu.env 关闭 dashboard 弹出浏览器）。需要已在「工具 > tosu」配置 tosu。"
            label="随游戏自动启动 tosu"
            onChange={(value: boolean) =>
              void save({ ...settings, overlay_start_tosu_with_game: value })
            }
          />
        </div>
        <div className="mt-3">
          <Toggle
            checked={settings.overlay_show_with_game ?? false}
            description="默认关闭：覆盖层平时完全隐藏，按热键唤出。开启后游戏运行时在角落常驻一张点击穿透的状态卡（下载/补缺进度、未读聊天、IRC 异常）。"
            label="游戏运行时常驻 HUD 状态卡"
            onChange={(value: boolean) =>
              void save({ ...settings, overlay_show_with_game: value })
            }
          />
        </div>
        <div className="mt-3">
          <Toggle
            checked={settings.overlay_auto_hide_on_game_exit !== false}
            description="osu! 退出后隐藏覆盖层（不销毁，多人补缺监听继续）。"
            label="游戏退出时自动隐藏"
            onChange={(value: boolean) =>
              void save({ ...settings, overlay_auto_hide_on_game_exit: value })
            }
          />
        </div>
        <div className="mt-3">
          <Toggle
            checked={settings.overlay_click_through !== false}
            description="HUD 模式下鼠标事件穿透到游戏；关闭后可直接点击 HUD 上的按钮打开面板。"
            label="HUD 点击穿透"
            onChange={(value: boolean) => void save({ ...settings, overlay_click_through: value })}
          />
        </div>

        <label className="mt-5 block">
          <span className="mb-2 block text-sm font-medium text-slate-300">显示角落</span>
          <select
            className="w-full rounded-xl border border-white/[0.1] bg-[#0b101b] px-3 py-3 text-sm text-slate-200 outline-none focus:border-cyan-300/45"
            disabled={busy}
            onChange={(event) =>
              void save({ ...settings, overlay_corner: event.target.value as Corner })
            }
            value={settings.overlay_corner ?? "TopRight"}
          >
            {corners.map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
          <span className="mt-2 block text-xs leading-5 text-slate-500">
            面板模式下可以拖动标题栏临时移动，切换模式后会回到所选角落。
          </span>
        </label>
      </Card>

      <Card className="p-6">
        <SectionTitle
          title="唤起热键"
          description="全局快捷键（Steam 式）：隐藏时唤出 Direct 面板，面板打开时收起；面板内也可按 Esc 收起。"
        />
        <div className="mt-5 flex flex-wrap items-center gap-2">
          <Layers className="size-4 text-[var(--theme-primary)]" />
          <input
            className="min-w-0 flex-1 rounded-xl border border-white/[0.1] bg-[#0b101b] px-3 py-2.5 font-mono text-sm text-slate-200 outline-none focus:border-cyan-300/45"
            onChange={(event) => {
              setHotkeyDraft(event.target.value);
              setHotkeyDirty(true);
            }}
            placeholder="Control+Shift+O"
            spellCheck={false}
            value={hotkeyDraft}
          />
          <Button onClick={() => setRecording((value) => !value)} size="sm" variant="secondary">
            <Keyboard className="size-4" />
            {recording ? "录制中（Esc 取消）" : "录制"}
          </Button>
          <Button
            disabled={hotkeyBusy || !hotkeyDirty || !hotkeyDraft.trim()}
            onClick={() => void saveHotkey()}
            size="sm"
            variant="primary"
          >
            保存热键
          </Button>
        </div>
        {hotkeyError ? (
          <p className="mt-3 rounded-xl border border-amber-300/30 bg-amber-300/10 p-3 text-xs leading-5 text-amber-200">
            {hotkeyError}
          </p>
        ) : (
          <p className="mt-3 text-xs leading-5 text-slate-500">
            保存前会尝试注册以校验有效性；与其他程序冲突时会保留原热键。修饰键写法：Control / Shift / Alt / Meta。
          </p>
        )}
      </Card>

      <Card className="p-6">
        <SectionTitle
          title="全屏覆盖 · 游戏内渲染"
          description="把 OPP HUD 装入 tosu 的游戏内渲染（D3D/OpenGL 钩子直接绘制进游戏画面），独占全屏下也可见。"
        />
        <div className="mt-5">
          <Toggle
            checked={settings.overlay_ingame_enabled ?? false}
            description="安装后需要重启 tosu 生效；游戏内按 tosu 的覆盖热键（默认 Ctrl+Shift+Space）显示/隐藏。位置可在 tosu 网页面板中拖动调整。"
            label="启用 tosu 游戏内渲染"
            onChange={(value: boolean) =>
              void save({ ...settings, overlay_ingame_enabled: value })
            }
          />
        </div>
        <div className="mt-4 rounded-xl border border-white/[0.08] bg-white/[0.035] p-4 text-xs leading-6 text-slate-400">
          <p>
            安装状态：{status?.ingame_installed ? "已安装" : "未安装"}
            {status?.ingame_installed ? (
              <>
                {" · "}
                <button
                  className="text-[var(--theme-primary)] hover:underline"
                  onClick={() => void reinstallIngame()}
                  type="button"
                >
                  {ingameBusy ? "更新中…" : "重新安装（tosu 升级后如失效点此）"}
                </button>
              </>
            ) : null}
          </p>
          <p className="mt-1">
            需要 tosu 较新版本（内置 ingame overlay）；首次启用时 tosu 可能联网下载覆盖组件。置顶小窗方案仍可在无边框模式下使用，两者互不冲突。
          </p>
        </div>
      </Card>

      <Card className="p-6">
        <SectionTitle
          title="多人房间自动补缺"
          description="通过 osu! IRC 监听 #mp_ 频道的 BanchoBot 换图播报，本地缺失时自动经默认镜像下载并导入游戏。"
        />
        <div className="mt-5">
          <Toggle
            checked={settings.overlay_mp_auto_download !== false}
            description="关闭后仅提示缺失，不再自动下载。下载位置与镜像沿用「在线谱面」设置；未提交谱面会提示后跳过。"
            label="自动下载缺失谱面"
            onChange={(value: boolean) => void save({ ...settings, overlay_mp_auto_download: value })}
          />
        </div>

        <div className="mt-5 rounded-xl border border-white/[0.1] bg-white/[0.035] p-4">
          <p className="text-sm font-semibold text-slate-100">osu! IRC 凭据</p>
          <p className="mt-1 text-xs leading-5 text-slate-500">
            在 osu! 官网账户页的 IRC 区域生成令牌（与账户密码不同）。令牌仅保存到系统凭据管理器；
            osu! IRC 官方为明文认证，介意者请勿启用。
          </p>
          <div className="mt-3 flex flex-wrap gap-2">
            <input
              className="min-w-0 flex-1 rounded-xl border border-white/[0.1] bg-[#0b101b] px-3 py-2.5 text-sm text-slate-200 outline-none focus:border-cyan-300/45"
              onChange={(event) => setIrcUsername(event.target.value)}
              placeholder="osu! 用户名（IRC 昵称，空格用下划线）"
              spellCheck={false}
              value={ircUsername}
            />
            <input
              className="min-w-0 flex-1 rounded-xl border border-white/[0.1] bg-[#0b101b] px-3 py-2.5 text-sm text-slate-200 outline-none focus:border-cyan-300/45"
              onChange={(event) => setIrcToken(event.target.value)}
              placeholder={status?.irc_configured ? "IRC 令牌（已保存，留空则不修改）" : "IRC 令牌"}
              spellCheck={false}
              type="password"
              value={ircToken}
            />
            <Button
              disabled={ircBusy || (!ircToken.trim() && !ircUsername.trim())}
              onClick={() => void saveIrcCredentials()}
              size="sm"
              variant="primary"
            >
              保存凭据
            </Button>
            <a
              className="inline-flex min-h-8 items-center gap-1 rounded-xl border border-white/[0.08] px-3 text-xs text-slate-300 hover:bg-white/[0.05]"
              href="https://osu.ppy.sh/home/account/edit#irc"
              rel="noreferrer"
              target="_blank"
            >
              <ExternalLink className="size-3.5" />
              获取令牌
            </a>
          </div>
          <p className="mt-3 text-xs leading-5 text-slate-500">
            连接状态：{ircLabel}
            {status?.irc.state === "error" && status.irc.message ? ` · ${status.irc.message}` : ""}
          </p>
          {ircMessage ? (
            <p className="mt-2 rounded-xl border border-cyan-300/25 bg-cyan-300/10 p-3 text-xs leading-5 text-cyan-100">
              {ircMessage}
            </p>
          ) : null}
          <p className="mt-3 text-xs leading-5 text-slate-500">
            进入多人房间后自动发现并加入对应 #mp_ 频道；补缺监听在后端常驻，与覆盖层窗口无关。
          </p>
        </div>
      </Card>

      <Card className="p-6">
        <SectionTitle
          title="聊天翻译"
          description="多人房间聊天：收到的外文消息自动翻译；自己要发的话翻译成译文后复制，切回游戏粘贴发送。俚语/meme 建议用 LLM 服务。"
        />
        <div className="mt-5">
          <Toggle
            checked={settings.translation_enabled !== false}
            description="自动翻译收到的 #mp_ 频道消息：原文在上、译文在下；已是目标语言或纯符号时跳过。"
            label="自动翻译收到的消息"
            onChange={(value: boolean) => void save({ ...settings, translation_enabled: value })}
          />
        </div>

        <label className="mt-5 block">
          <span className="mb-2 block text-sm font-medium text-slate-300">翻译服务</span>
          <select
            className="w-full rounded-xl border border-white/[0.1] bg-[#0b101b] px-3 py-3 text-sm text-slate-200 outline-none focus:border-cyan-300/45"
            disabled={busy}
            onChange={(event) =>
              void save({ ...settings, translation_provider: event.target.value })
            }
            value={translationProvider}
          >
            {translationProviders.map((provider) => (
              <option key={provider.value} value={provider.value}>
                {provider.label}
              </option>
            ))}
          </select>
          <span className="mt-2 block text-xs leading-5 text-slate-500">
            {translationProviders.find((provider) => provider.value === translationProvider)?.hint}
          </span>
        </label>

        <div className="mt-4 grid grid-cols-2 gap-3">
          <label className="block">
            <span className="mb-2 block text-sm font-medium text-slate-300">收信翻译成</span>
            <select
              className="w-full rounded-xl border border-white/[0.1] bg-[#0b101b] px-3 py-2.5 text-sm text-slate-200 outline-none focus:border-cyan-300/45"
              disabled={busy}
              onChange={(event) =>
                void save({ ...settings, translation_target_lang: event.target.value })
              }
              value={settings.translation_target_lang ?? "zh"}
            >
              {targetLanguages.map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <label className="block">
            <span className="mb-2 block text-sm font-medium text-slate-300">发言翻译成</span>
            <select
              className="w-full rounded-xl border border-white/[0.1] bg-[#0b101b] px-3 py-2.5 text-sm text-slate-200 outline-none focus:border-cyan-300/45"
              disabled={busy}
              onChange={(event) =>
                void save({ ...settings, translation_outgoing_target_lang: event.target.value })
              }
              value={settings.translation_outgoing_target_lang ?? "en"}
            >
              {targetLanguages.map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
        </div>

        {needsCredential ? (
          <div className="mt-4 rounded-xl border border-white/[0.1] bg-white/[0.035] p-4">
            <p className="text-sm font-semibold text-slate-100">服务凭据</p>
            {translationProvider === "openai" ? (
              <>
                <div className="mt-2 flex flex-wrap gap-2">
                  {openaiPresets.map((preset) => (
                    <button
                      key={preset.label}
                      className="rounded-xl border border-white/[0.08] px-3 py-1.5 text-xs text-slate-300 hover:bg-white/[0.05]"
                      onClick={() => {
                        setApiBase(preset.base);
                        setApiModel(preset.model);
                      }}
                      type="button"
                    >
                      {preset.label}
                    </button>
                  ))}
                </div>
                <div className="mt-3 flex flex-wrap gap-2">
                  <input
                    className="min-w-0 flex-1 rounded-xl border border-white/[0.1] bg-[#0b101b] px-3 py-2.5 text-sm text-slate-200 outline-none focus:border-cyan-300/45"
                    onChange={(event) => setApiBase(event.target.value)}
                    placeholder="API 地址（如 https://open.bigmodel.cn/api/paas/v4）"
                    spellCheck={false}
                    value={apiBase}
                  />
                  <input
                    className="min-w-0 rounded-xl border border-white/[0.1] bg-[#0b101b] px-3 py-2.5 text-sm text-slate-200 outline-none focus:border-cyan-300/45"
                    onChange={(event) => setApiModel(event.target.value)}
                    placeholder="模型（glm-4-flash）"
                    spellCheck={false}
                    value={apiModel}
                  />
                  <Button
                    disabled={busy || !apiBase.trim()}
                    onClick={() =>
                      void save({
                        ...settings,
                        translation_api_base_url: apiBase.trim() || null,
                        translation_api_model: apiModel.trim() || null,
                      })
                    }
                    size="sm"
                    variant="primary"
                  >
                    保存服务配置
                  </Button>
                </div>
                <p className="mt-2 text-xs leading-5 text-slate-500">
                  GLM-4-Flash 在智谱 bigmodel.cn 注册后免费使用；Ollama 本地服务无需真实 Key（填 ollama 即可）。
                </p>
              </>
            ) : null}
            <div className="mt-3 flex flex-wrap gap-2">
              <input
                className="min-w-0 flex-1 rounded-xl border border-white/[0.1] bg-[#0b101b] px-3 py-2.5 text-sm text-slate-200 outline-none focus:border-cyan-300/45"
                onChange={(event) => setCredentialPrimary(event.target.value)}
                placeholder={
                  translationProvider === "tencent"
                    ? "SecretId"
                    : translationProvider === "baidu"
                      ? "APP ID"
                      : "API Key"
                }
                spellCheck={false}
                type="password"
                value={credentialPrimary}
              />
              {translationProvider === "tencent" || translationProvider === "baidu" ? (
                <input
                  className="min-w-0 flex-1 rounded-xl border border-white/[0.1] bg-[#0b101b] px-3 py-2.5 text-sm text-slate-200 outline-none focus:border-cyan-300/45"
                  onChange={(event) => setCredentialSecondary(event.target.value)}
                  placeholder={translationProvider === "tencent" ? "SecretKey" : "密钥"}
                  spellCheck={false}
                  type="password"
                  value={credentialSecondary}
                />
              ) : null}
              <Button
                disabled={credentialBusy}
                onClick={() => void saveTranslationCredential()}
                size="sm"
                variant="primary"
              >
                保存凭据
              </Button>
            </div>
            <p className="mt-2 text-xs leading-5 text-slate-500">
              凭据仅保存到系统密钥环；两框留空点「保存凭据」即清除已保存内容。
              {translationProvider === "tencent" ? (
                <>
                  {" "}
                  <a
                    className="text-[var(--theme-primary)] hover:underline"
                    href="https://console.cloud.tencent.com/cam/capi"
                    rel="noreferrer"
                    target="_blank"
                  >
                    获取密钥
                  </a>
                </>
              ) : null}
              {translationProvider === "baidu" ? (
                <>
                  {" "}
                  <a
                    className="text-[var(--theme-primary)] hover:underline"
                    href="https://fanyi-api.baidu.com/"
                    rel="noreferrer"
                    target="_blank"
                  >
                    开放平台
                  </a>
                </>
              ) : null}
            </p>
            {credentialMessage ? (
              <p className="mt-2 rounded-xl border border-cyan-300/25 bg-cyan-300/10 p-3 text-xs leading-5 text-cyan-100">
                {credentialMessage}
              </p>
            ) : null}
          </div>
        ) : null}

        <div className="mt-4 flex flex-wrap items-center gap-3">
          <Button
            disabled={testBusy || translationProvider === "off" || busy}
            onClick={() => void runTranslationTest()}
            size="sm"
            variant="secondary"
          >
            {testBusy ? "测试中…" : "测试翻译"}
          </Button>
          {testMessage ? (
            <span className="min-w-0 flex-1 text-xs leading-5 text-slate-400">{testMessage}</span>
          ) : null}
        </div>

        <p className="mt-4 text-xs leading-5 text-slate-500">
          聊天内容会发送到所配置的翻译服务处理，请勿对不希望第三方接触的内容开启本功能。
        </p>
      </Card>

      <Card className="p-6">
        <SectionTitle title="已知限制" description="覆盖层为独立置顶窗口 + tosu 游戏内渲染双通道。" />
        <ul className="mt-4 list-disc space-y-2 pl-5 text-xs leading-6 text-slate-400">
          <li>置顶小窗在独占全屏下不可见：请启用上面的「游戏内渲染」，或以无边框运行 osu!。</li>
          <li>Direct 面板搜索需要已在 OPP 登录 osu! 账户（与主窗口在线谱面一致）。</li>
          <li>当前版本在 Windows 上验证；Linux 需要 X11（Wayland 不支持透明置顶窗），待实测。</li>
        </ul>
      </Card>
    </div>
  );
}
