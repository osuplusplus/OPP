import "@testing-library/jest-dom/vitest";

// vitest 4 + jsdom 29 组合下 `window.localStorage` 为 undefined(jsdom 将其
// 实现为 window 实例上的非枚举属性,vitest 同步全局时被跳过),组件里的
// 偏好读取(ModeContext 等)会直接抛错。这里兜底一个内存实现,行为与
// 真实 localStorage 一致且测试间互不污染(vitest 每文件新环境)。
if (typeof window !== "undefined" && window.localStorage === undefined) {
  const store = new Map<string, string>();
  const storage: Storage = {
    get length() {
      return store.size;
    },
    clear: () => store.clear(),
    getItem: (key) => (store.has(key) ? store.get(key)! : null),
    key: (index) => Array.from(store.keys())[index] ?? null,
    removeItem: (key) => {
      store.delete(key);
    },
    setItem: (key, value) => {
      store.set(key, String(value));
    },
  };
  Object.defineProperty(window, "localStorage", {
    value: storage,
    configurable: true,
    writable: true,
  });
}
