# Solução de problemas

[English](troubleshooting.md)

## O aplicativo ou projeto não abre

Confirme que a pasta existe e pode ser lida. Se foi movida, use **Localizar
pasta** e autorize a nova raiz. O histórico local permanece legível enquanto a
raiz antiga está indisponível, mas arquivos, Git e IA ficam bloqueados até nova
seleção explícita.

## Nenhuma IA selecionada ou modelo ausente

Abra **Configurações → IA**, conecte um Provider e escolha o modelo do
workspace. Enquanto nenhum modelo estiver associado, o Nocturne recusa a
execução com uma mensagem clara, sem recorrer a outra IA. Se o Provider não
listar modelos, verifique a chave e o endpoint e atualize a lista pelo botão de
modelo da conexão.

## Um Provider falha

Verifique endpoint, regra de HTTPS/loopback, credencial e catálogo de modelos.
Para Ollama ou LM Studio, inicie o serviço local. O diagnóstico diferencia
autenticação, créditos, rate limit, timeout, endpoint indisponível e resposta
inválida.

## Uma execução foi interrompida

Leia o resumo do erro para saber o que foi preservado e use **Tentar novamente**
quando a operação permitir. Rollback de Build só aparece quando snapshot e
limites dos arquivos reportados permitem uma reversão segura.

## Recovery ou atualização falhou

Não apague o banco local nem artefatos de recuperação. Use o recovery guiado ou
importe um backup verificado. Para download interrompido, retome; a versão atual
e os dados locais continuam sendo a base até o novo pacote ser validado.

## Diagnóstico

Use **Configurações > Diagnóstico** para copiar ou exportar um relatório
sanitizado. Revise qualquer log antes de compartilhá-lo. Relatórios não devem
conter credenciais, prompts, conteúdo do projeto, diffs ou caminhos privados.
