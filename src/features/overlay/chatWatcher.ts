import { useCallback, useEffect, useRef, useState } from "react";
import { desktopApi } from "../../shared/lib/tauri";
import type { OverlayChatEntry, OverlayChatUpdate } from "../../shared/types/osu";

/** 初始拉取的历史条数与前端缓冲上限（后端缓冲 300 条）。 */
const CHAT_HISTORY_LIMIT = 120;
const CHAT_BUFFER_LIMIT = 200;

export interface ChatState {
  entries: OverlayChatEntry[];
  /** 聊天页不可见期间收到的新消息数（HUD 徽标展示）。 */
  unread: number;
  markRead: () => void;
}

/** 追加消息并裁剪前端缓冲。 */
export function appendChatEntry(
  entries: OverlayChatEntry[],
  entry: OverlayChatEntry,
): OverlayChatEntry[] {
  const next = [...entries, entry];
  return next.length > CHAT_BUFFER_LIMIT ? next.slice(next.length - CHAT_BUFFER_LIMIT) : next;
}

/** 按 id 原位回填译文；未命中（已淘汰）时忽略。 */
export function applyChatUpdate(
  entries: OverlayChatEntry[],
  update: OverlayChatUpdate,
): OverlayChatEntry[] {
  return entries.map((entry) =>
    entry.id === update.id ? { ...entry, translation: update.translation } : entry,
  );
}

/**
 * MP 聊天订阅：初始拉取缓冲 + 事件增量合并。缓冲在后端与窗口生命周期无关，
 * 这里只负责展示同步；`visible` 表示聊天页当前是否可见（控制未读计数）。
 */
export function useChat(visible: boolean): ChatState {
  const [entries, setEntries] = useState<OverlayChatEntry[]>([]);
  const [unread, setUnread] = useState(0);
  const visibleRef = useRef(visible);

  // 可见性供事件回调读取（回调闭包拿不到最新 state）；只在 effect 中写 ref。
  useEffect(() => {
    visibleRef.current = visible;
  }, [visible]);

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    void desktopApi
      .overlayGetChat(CHAT_HISTORY_LIMIT)
      .then((history) => {
        if (!disposed) setEntries(history);
      })
      .catch(() => {});
    void desktopApi.onOverlayChatMessage((entry) => {
      setEntries((previous) => appendChatEntry(previous, entry));
      // 聊天页可见时新消息即视为已读，否则计入 HUD 徽标（系统消息不计）。
      if (visibleRef.current) {
        setUnread(0);
      } else if (entry.kind !== "system") {
        setUnread((count) => count + 1);
      }
    }).then((unlisten) => {
      if (disposed) unlisten();
      else unlisteners.push(unlisten);
    });
    void desktopApi.onOverlayChatUpdated((update) => {
      setEntries((previous) => applyChatUpdate(previous, update));
    }).then((unlisten) => {
      if (disposed) unlisten();
      else unlisteners.push(unlisten);
    });
    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  const markRead = useCallback(() => setUnread(0), []);

  return { entries, unread, markRead };
}
