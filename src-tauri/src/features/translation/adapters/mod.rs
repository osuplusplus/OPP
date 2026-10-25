//! 外部翻译服务的适配器集合：每家 API 一个模块，彼此独立。

// Google 免费翻译端点（零配置，国内需代理）。
pub(crate) mod google_free;
// OpenAI 兼容 LLM（GLM/DeepSeek/OpenAI/Ollama，俚语质量最佳）。
pub(crate) mod openai_compat;
// 百度翻译开放平台（MD5 签名，国内直连）。
pub(crate) mod baidu;
// 腾讯云机器翻译 TMT（TC3 签名，国内直连）。
pub(crate) mod tencent;
