# Modos Review, Build e Docs

[English](modes.md)

## Review Mode

Review é somente análise. Lê o workspace autorizado e produz sugestões
estruturadas com evidência, confiança, origem, severidade, justificativa e
histórico de decisões. Uma nova análise reconcilia achados novos e persistentes
e resolve achados abertos que não são sustentados pelas evidências atuais.
Review nunca altera arquivos por conta própria.

## Build Mode

Build pode modificar arquivos somente dentro do workspace autorizado e sob a
política de aprovação ativa. Com um modelo de Provider, o agente usa as
ferramentas de workspace do Nocturne: escritas ficam dentro da raiz autorizada
(diretórios gerados, `.git`, `.nocturne` e prováveis segredos são recusados) e
todo comando exige aprovação explícita e roda sem shell. Progresso, aprovações,
arquivos alterados e diffs permanecem
visíveis.

Rollback protegido só é oferecido quando o workspace estava limpo antes da
execução, existe commit `HEAD` e o agente reportou caminhos contidos na raiz
autorizada. Revise o diff antes de confirmar o rollback.

## Docs Mode

Docs gera propostas somente leitura. O usuário escolhe um arquivo Markdown,
compara conteúdo atual e proposto e confirma anexar, substituir ou criar. O hash
é verificado novamente antes da escrita, que é atômica. Exports HTML, DOCX e PDF
são cópias derivadas e não atualizações incrementais do documento-fonte.

Recursos avançados de automação autônoma, build e orquestração ficam fora do
contrato atual da 1.0.

## Agentes dos modos

Cada modo tem um agente, listado em Ajustes › Agentes. Um agente tem um nome que segue o modo, uma instrução (texto, até 4 KiB)
e um modelo. Modelo vazio significa o modelo do projeto. "Restaurar padrão" devolve o agente à instrução padrão e ao modelo do
projeto.

- O agente de Revisar responde o chat em Revisar. Sua instrução padrão é a frase que o chat sempre usou. As regras que o shell
  acrescenta depois da instrução não fazem parte dela e não podem ser editadas. Revisar nunca grava alteração: a resposta ali é
  texto e achados, e nada fica pendente.
- Os agentes de Construir e Documentar respondem o chat nesses modos, pelo botão de conversa da barra superior. Uma resposta
  completa pode propor alterações; cada uma espera a aprovação da pessoa, e o shell a registra pela regra do seu modo:
  - Construir: qualquer caminho que as regras de caminho compartilhadas aceitam.
  - Documentar: somente arquivos Markdown (`.md` ou `.markdown`). O shell recusa qualquer outro caminho, e a resposta mostra o
    motivo.
- "Gerar alteração", nas sugestões de Revisar, é uma etapa de implementação: a alteração espera em Construir, sob a regra de
  Construir. Ela não usa a instrução de um agente.
- As permissões pertencem ao modo e o shell as aplica. Uma instrução ou um modelo não as amplia, e um agente não tem campo de
  permissão.
- Limitação conhecida: aprovar uma alteração em Documentar não é recusada pelo shell quando a alteração não é Markdown. A lista
  de Documentar mostra só Markdown, e esse filtro está na interface.
