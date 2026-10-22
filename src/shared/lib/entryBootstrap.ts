/**
 * 多窗口入口（主窗口／迷你播放器／覆盖层）的公共引导逻辑：
 * 从设置初始化主题变量、屏蔽浏览器右键菜单。
 */

interface EntryThemeSettings {
  theme_mode?: string | null;
  theme_primary?: string | null;
}

/** 一次性应用持久化的主题；读取失败时保持内置默认值。 */
export function applyEntryTheme(
  settings: Promise<EntryThemeSettings | null | undefined>,
): void {
  void settings
    .then((value) => {
      const root = document.documentElement;
      root.dataset.themeMode = value?.theme_mode ?? "dark";
      root.dataset.themePrimary = value?.theme_primary ?? "cyan";
    })
    .catch(() => {});
}

/** 屏蔽桌面应用中的浏览器右键菜单，返回解除函数。 */
export function blockContextMenu(target: Document | Window = document): () => void {
  const block = (event: Event) => event.preventDefault();
  target.addEventListener("contextmenu", block);
  return () => target.removeEventListener("contextmenu", block);
}
