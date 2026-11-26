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

- O agente de Revisão responde o chat. Sua instrução padrão é a frase que o chat sempre usou. As regras que o shell acrescenta
  depois da instrução (sem comandos, sem gravação, formato da resposta, no máximo cinco arquivos) não fazem parte dela e não
  podem ser editadas. Como no próprio modo Revisar, uma resposta completa pode deixar alterações propostas como pendentes, para
  a pessoa aprovar.
- Os agentes de Construção e Documentação podem ser editados e ficam guardados, mas nenhuma chamada os usa ainda. A geração de
  alterações continua saindo de Revisar, com o prompt próprio.
- As permissões pertencem ao modo e o shell as aplica. Uma instrução ou um modelo não as amplia, e um agente não tem campo de
  permissão.
- Limitação conhecida: em Documentar só aparecem alterações Markdown, e esse filtro está na interface. O shell não recusa outro
  tipo de alteração quando ela é aprovada.
