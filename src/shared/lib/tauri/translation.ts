import { call } from "./invoke";

// translation 域的 Tauri 命令适配：聊天翻译（出站翻译/凭据/连通性测试）。

/** 单条文本的翻译结果。 */
export interface TranslationOutcome {
  text: string;
  provider: string;
  detected_lang: string;
}

/** 连通性测试结果。 */
export interface TranslationTestResult {
  provider: string;
  target: string;
  translated_text: string;
}

export const translationApi = {
  /** 出站翻译：聊天输入框文本 → 目标语言（缺省取设置的出站目标语言）。 */
  translateTexts: (texts: string[], targetLang?: string) =>
    call<TranslationOutcome[]>("translate_texts", {
      texts,
      targetLang: targetLang ?? null,
    }),

  /** 保存/清除翻译服务凭据（仅入系统密钥环；null 或空串为清除）。 */
  translationSetCredential: (credential: string | null) =>
    call<void>("translation_set_credential", {
      credential: credential && credential.trim() ? credential.trim() : null,
    }),

  /** 翻译样例验证配置连通性（设置页测试按钮）。 */
  translationTest: () => call<TranslationTestResult>("translation_test"),
};
