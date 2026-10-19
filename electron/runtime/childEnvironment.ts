// Environment passed to user-triggered child processes (validation commands and
// agent-requested commands). Only an allowlist crosses the boundary, and
// credentials are denied even when they appear in the allowlist by mistake.
const CHILD_ENVIRONMENT_ALLOWLIST = new Set([
  'PATH',
  'SHELL',
  'COMSPEC',
  'SYSTEMROOT',
  'WINDIR',
  'PATHEXT',
  'HOME',
  'USERPROFILE',
  'HOMEDRIVE',
  'HOMEPATH',
  'APPDATA',
  'LOCALAPPDATA',
  'XDG_CONFIG_HOME',
  'XDG_DATA_HOME',
  'XDG_CACHE_HOME',
  'TMPDIR',
  'TMP',
  'TEMP',
  'LANG',
  'LC_ALL',
  'LC_CTYPE',
  'TERM',
  'COLORTERM',
  'NO_COLOR',
  'SSL_CERT_FILE',
  'SSL_CERT_DIR',
])

const CHILD_ENVIRONMENT_DENYLIST = new Set([
  'OPENAI_API_KEY',
  'ANTHROPIC_API_KEY',
  'GEMINI_API_KEY',
  'GOOGLE_API_KEY',
  'OPENROUTER_API_KEY',
  'DEEPSEEK_API_KEY',
  'GROQ_API_KEY',
  'MISTRAL_API_KEY',
  'XAI_API_KEY',
  'NODE_OPTIONS',
  'NODE_DEBUG',
  'NODE_EXTRA_CA_CERTS',
  'ELECTRON_RUN_AS_NODE',
  'ELECTRON_ENABLE_LOGGING',
  'HTTP_PROXY',
  'HTTPS_PROXY',
  'ALL_PROXY',
  'NO_PROXY',
  'SSH_AUTH_SOCK',
  'SSH_AGENT_PID',
  'SSH_ASKPASS',
  'GIT_ASKPASS',
  'GIT_SSH_COMMAND',
])

const SENSITIVE_ENVIRONMENT_KEY = /(?:^|_)(?:API_?KEY|ACCESS_TOKEN|AUTH_TOKEN|SECRET|PASSWORD|PASSWD|PRIVATE_KEY|CREDENTIALS?)(?:_|$)/i

export function buildChildEnvironment(source: Record<string, string | undefined> = process.env): NodeJS.ProcessEnv {
  const environment = {} as NodeJS.ProcessEnv
  for (const [key, value] of Object.entries(source)) {
    if (value === undefined) continue
    const normalizedKey = key.toUpperCase()
    if (!CHILD_ENVIRONMENT_ALLOWLIST.has(normalizedKey)) continue
    if (CHILD_ENVIRONMENT_DENYLIST.has(normalizedKey) || SENSITIVE_ENVIRONMENT_KEY.test(normalizedKey)) continue
    environment[key] = value
  }
  return environment
}
