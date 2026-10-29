# Contrato de Providers

Todo adapter registrado no NOCT declara o mesmo contrato interno:

- identidade, origem, protocolo e versão quando disponível;
- descoberta de modelos;
- suporte a streaming, ferramentas e cancelamento;
- modo de autenticação;
- limites de timeout e limitações conhecidas;
- disponibilidade normalizada;
- execução com `AbortSignal` e eventos de streaming normalizados.

O diagnóstico mede a latência da verificação, separa conectividade,
autenticação e compatibilidade e mantém até cinco erros recentes sanitizados
por Provider durante a sessão. Credenciais, respostas nativas e payloads não
entram no diagnóstico.

O adapter Chat Completions oferece descoberta de modelos, streaming,
cancelamento, embeddings e chamadas de ferramentas normalizadas. O main process
executa um loop limitado: Review e Docs usam ferramentas de leitura; Build
também pode escrever e pedir aprovação explícita para comandos. Review e Docs
podem tentar uma vez sem ferramentas quando o modelo recusa function calling. Falhas HTTP e de protocolo são convertidas em códigos comuns:
autenticação recusada, créditos insuficientes, rate limit, modelo indisponível,
Provider inacessível, resposta inválida, timeout e cancelamento.
