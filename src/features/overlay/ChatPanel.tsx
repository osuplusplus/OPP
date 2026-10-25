import { useEffect, useMemo, useRef, useState } from "react";
import { desktopApi, type TranslationOutcome } from "../../shared/lib/tauri";
import { errorMessage } from "../../shared/lib/format";
import type { AppSettings, OverlayChatEntry } from "../../shared/types/osu";
import type { OverlayIrcState } from "./mpWatcher";
import { chatNickColor, ircNotice } from "./model";

/** 出站翻译目标语言选项（内部语言码，与后端语言表一致）。 */
const OUTGOING_LANGUAGES: Array<{ code: string; label: string }> = [
  { code: "en", label: "英语" },
  { code: "ja", label: "日语" },
  { code: "ko", label: "韩语" },
  { code: "ru", label: "俄语" },
  { code: "es", label: "西语" },
  { code: "pt", label: "葡语" },
  { code: "zh", label: "中文" },
];

const COPIED_FEEDBACK_MS = 1500;

/**
 * MP 频道聊天面板：消息流（原文 + 自动翻译译文）与"输入 → 翻译 → 复制"的
 * 发送辅助。消息不经 OPP 发出——复制译文后切回游戏聊天粘贴发送。顶部横幅
 * 同步 IRC 连接状态，未配置/认证失败时给出可操作的指引。
 */
export function ChatPanel({
  settings,
  entries,
  irc,
}: {
  settings: AppSettings | null;
  entries: OverlayChatEntry[];
  irc: OverlayIrcState | null;
}) {
  const notice = ircNotice(irc);
  const channels = useMemo(
    () => Array.from(new Set(entries.map((entry) => entry.channel))),
    [entries],
  );
  const [selectedChannel, setSelectedChannel] = useState("");
  const activeChannel =
    selectedChannel && channels.includes(selectedChannel)
      ? selectedChannel
      : (channels[channels.length - 1] ?? "");
  const visibleEntries = activeChannel
    ? entries.filter((entry) => entry.channel === activeChannel)
    : entries;

  const [input, setInput] = useState("");
  const [targetLang, setTargetLang] = useState(
    settings?.translation_outgoing_target_lang ?? "en",
  );
  const [translating, setTranslating] = useState(false);
  const [result, setResult] = useState<{ source: string; translated: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const copiedTimer = useRef<number | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);

  // 新消息到达时保持滚动到底部。
  useEffect(() => {
    const list = listRef.current;
    if (list) list.scrollTop = list.scrollHeight;
  }, [visibleEntries.length]);

  useEffect(
    () => () => {
      if (copiedTimer.current != null) window.clearTimeout(copiedTimer.current);
    },
    [],
  );

  const translate = async () => {
    const text = input.trim();
    if (!text || translating) return;
    setTranslating(true);
    setError(null);
    setCopied(false);
    try {
      const outcomes: TranslationOutcome[] = await desktopApi.translateTexts([text], targetLang);
      const translated = outcomes[0]?.text ?? "";
      if (!translated) {
        setError("翻译未返回结果，请重试");
        return;
      }
      setResult({ source: text, translated });
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setTranslating(false);
    }
  };

  const copyText = async (text: string) => {
    try {
      await desktopApi.overlayCopyText(text);
      setCopied(true);
      if (copiedTimer.current != null) window.clearTimeout(copiedTimer.current);
      copiedTimer.current = window.setTimeout(() => setCopied(false), COPIED_FEEDBACK_MS);
    } catch (caught) {
      setError(errorMessage(caught));
    }
  };

  return (
    <>
      {notice ? <div className={`chat-irc chat-irc-${notice.tone}`}>{notice.text}</div> : null}
      <div className="chat-channels">
        {channels.length > 1
          ? channels.map((channel) => (
              <button
                key={channel}
                type="button"
                className={`chat-channel${channel === activeChannel ? " active" : ""}`}
                onClick={() => setSelectedChannel(channel)}
              >
                {channel}
              </button>
            ))
          : (
              <span className={`chat-channel${activeChannel ? " active" : ""}`}>
                {activeChannel || "等待 MP 房间…"}
              </span>
            )}
      </div>
      <div className="chat-list" ref={listRef}>
        {visibleEntries.length === 0 ? (
          <div className="panel-hint">进入多人房间后，这里会显示 #mp 频道聊天与自动翻译。</div>
        ) : null}
        {visibleEntries.map((entry) => (
          <div
            key={entry.id}
            className={`chat-entry${entry.kind === "system" ? " chat-system" : ""}`}
          >
            <span className="chat-nick" style={{ color: chatNickColor(entry.nick) }}>
              {entry.nick}
            </span>
            <span className="chat-text">
              {entry.text}
              {entry.translation ? (
                <span className="chat-translation">{entry.translation}</span>
              ) : null}
            </span>
          </div>
        ))}
      </div>
      <div className="chat-compose">
        {result ? (
          <div className="chat-result">
            <div className="chat-result-text" title={result.source}>
              {result.translated}
            </div>
            <button type="button" className="chat-copy" onClick={() => void copyText(result.translated)}>
              {copied ? "已复制 ✓" : "复制"}
            </button>
            <button
              type="button"
              className="chat-copy chat-copy-plain"
              onClick={() => void copyText(result.source)}
            >
              原文
            </button>
          </div>
        ) : null}
        {error ? <div className="panel-error">{error}</div> : null}
        <div className="chat-input-row">
          <input
            className="chat-input"
            value={input}
            placeholder="输入要说的话，回车翻译"
            onChange={(event) => setInput(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void translate();
            }}
            autoFocus
            spellCheck={false}
          />
          <select
            className="panel-select"
            value={targetLang}
            onChange={(event) => setTargetLang(event.target.value)}
          >
            {OUTGOING_LANGUAGES.map((language) => (
              <option key={language.code} value={language.code}>
                {language.label}
              </option>
            ))}
          </select>
          <button
            type="button"
            className="chat-translate"
            disabled={translating || !input.trim()}
            onClick={() => void translate()}
          >
            {translating ? "翻译中…" : "翻译"}
          </button>
        </div>
        <div className="chat-hint">复制译文后切回游戏，在聊天框 Ctrl+V 粘贴发送。</div>
      </div>
    </>
  );
}
