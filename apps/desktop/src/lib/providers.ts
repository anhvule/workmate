/**
 * Providers workmate offers by name. The engine knows more; this is the short
 * list a person is likely to look for, and anything else can be typed.
 */
export const COMMON_PROVIDERS: { id: string; name: string }[] = [
  { id: "anthropic", name: "Anthropic" },
  { id: "openai", name: "OpenAI" },
  { id: "google", name: "Google" },
  { id: "openrouter", name: "OpenRouter" },
  { id: "mistral", name: "Mistral" },
  { id: "groq", name: "Groq" },
  { id: "xai", name: "xAI" },
  { id: "deepseek", name: "DeepSeek" },
];

export const providerName = (id: string): string => COMMON_PROVIDERS.find((p) => p.id === id)?.name ?? id;
