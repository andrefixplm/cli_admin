# Servidor MCP — `cli_admin_mcp`

Servidor do **Model Context Protocol** (revisão **2026-07-28**) que expõe ao agente de IA um subconjunto seguro do `cli_admin_v3`. Transporte: **stdio** (o cliente inicia o processo). Não há porta de rede.

Para um passo a passo de integração feito para ser seguido por uma LLM, veja a skill `cli-admin-mcp` em `.claude/skills/cli-admin-mcp/SKILL.md`.

## Compilar

```powershell
cargo build --release --offline --features mcp
# -> target\release\cli_admin_mcp.exe
```

Sem `--features mcp`, o binário não é compilado e o `cli_admin_v3.exe` não muda.

Instalação local usada neste projeto: copie `target\release\cli_admin_mcp.exe` e `cli_admin_v3.exe` para `c:\local\bin`, versionando a cópia anterior antes de sobrescrever.

## Executar

```powershell
C:\local\bin\cli_admin_mcp.exe --config C:\local\share\cli_admin_mcp\config.toml
```

| Opção | Efeito |
|---|---|
| `--config <arquivo>` | Usa esta configuração (mesma precedência do app) |
| `--permitir-elevado` | Permite iniciar como administrador. **Sem essa opção, o servidor recusa** e sai com código `2` |

Regras de execução:

- **stdout é do protocolo.** Nenhuma linha de log vai para o stdout. Logs vão para o arquivo de `general.log_file`.
- Iniciar **a partir de um terminal elevado é recusado**. Use um terminal comum. Se o cliente estiver numa sessão elevada, a conexão cai com `Connection closed`.

## Registrar no cliente

O agente não se conecta sozinho: o **cliente** (opencode, Claude Code etc.) inicia o processo e lista as ferramentas. Basta registrar o servidor uma vez na configuração do cliente.

### opencode

Configuração global: `%USERPROFILE%\.config\opencode\opencode.jsonc`.

```jsonc
"mcp": {
  "cli_admin": {
    "type": "local",
    "command": ["C:\\local\\bin\\cli_admin_mcp.exe", "--config", "C:\\local\\share\\cli_admin_mcp\\config.toml"],
    "enabled": true,
    "timeout": 15000
  }
}
```

Verificar: `opencode mcp list` deve mostrar `cli_admin connected`.

### Claude Code

```powershell
claude mcp add cli_admin -- "C:\local\bin\cli_admin_mcp.exe" --config "C:\local\share\cli_admin_mcp\config.toml"
```

Verificar: `claude mcp list`. Confira a sintaxe com `claude mcp --help` se o comando mudar na versão instalada.

### Outros clientes

Em geral basta indicar o comando e os argumentos do servidor stdio. Confira a documentação do cliente escolhido.

## Configuração `[mcp]`

Exemplo mínimo para este ambiente:

```toml
[mcp]
allow_apply = true                                        # liga as ferramentas de aplicação
pasta_planos = "C:\\local\\share\\cli_admin_mcp\\planos"
auditoria = "C:\\local\\share\\cli_admin_mcp\\audit\\mcp.log"
raizes_permitidas = ["C:\\Users\\Andre"]                  # usado só por fs_listar
```

| Chave | Padrão | Efeito |
|---|---|---|
| `allow_apply` | `false` | Liga `plano_aplicar` e `servico_aplicar`. Desligado, essas ferramentas nem aparecem |
| `expor_linha_de_comando` | `true` | `false` omite o campo em `processos_listar` |
| `raizes_permitidas` | pasta do perfil do usuário | Pastas que `fs_listar` pode listar. Lista vazia = recusa tudo |
| `caminhos_negados` | `.ssh`, `.aws`, `.azure`, `.gnupg`, `.kube`, `.docker`, `.git-credentials`, `*.pem`, `*.pfx`, `*.key`, `.env`, `NTUSER.DAT`, `Login Data`, `Cookies`, `Credentials` | Componentes ou padrões bloqueados em qualquer ponto do caminho |
| `limite_por_minuto` | `60` | Chamadas por minuto na sessão |
| `limite_sessao` | `1000` | Chamadas totais na sessão |
| `max_resposta_bytes` | `65536` | Teto de cada resposta |
| `max_campo` | `512` | Tamanho máximo de cada campo de texto vindo do sistema |
| `validade_plano_s` | `300` | Validade de um plano, em segundos |
| `pasta_planos` | `mcp/pendentes` | Onde os planos ficam. **Use caminho absoluto** |
| `auditoria` | `audit/mcp.log` | Arquivo de auditoria. **Use caminho absoluto** |

> Chaves reservadas, **sem efeito ainda**: `db_allow_login` e `hosts_permitidos`. As ferramentas que as usariam ainda não existem.

**Importante:** o servidor MCP e a TUI (`cli_admin_v3`) precisam apontar para a **mesma** `pasta_planos`. É ali que a TUI lista e aprova os planos com `F2`.

## Ferramentas

| Ferramenta | Nível | Descrição |
|---|---|---|
| `processos_listar` | Leitura | Processos com CPU, memória, usuário, caminho e linha de comando redigida. `filtro`, `ordenar` (`cpu`, `ram`, `nome`), `limite` (até 200), `offset` |
| `portas_listar` | Leitura | Portas TCP/UDP e o processo dono. `so_escuta`, `filtro` (nome ou porta), `limite`, `offset` |
| `fs_listar` | Leitura | Nomes e tamanhos de uma pasta **dentro das raízes permitidas**. Não lê conteúdo |
| `servicos_listar` | Leitura | Serviços do Windows: nome, nome de exibição, estado, tipo de início e se é crítico. `filtro`, `estado` (`running`, `stopped`, `paused`), `limite`, `offset` |
| `servico_detalhar` | Leitura | Detalhes de um serviço (nome exato): caminho, conta, dependências e dependentes ativos |
| `processo_planejar_encerramento` | Planejamento | Recebe até 50 PIDs e cria um plano. Protegidos e inexistentes ficam de fora. Devolve `plano_id` |
| `servico_planejar_acao` | Planejamento | Cria um plano para `start` ou `stop` de um serviço (parar inclui os dependentes ativos). Serviços críticos são recusados. Devolve `plano_id` e os passos |
| `plano_aplicar` | Aplicação | Só com `allow_apply = true`. Recebe `plano_id` e **os mesmos PIDs** |
| `servico_aplicar` | Aplicação | Só com `allow_apply = true`. Recebe `plano_id`, `nome` e `acao` iguais aos do plano |

Não existe ferramenta para executar comando livre, SQL ou ler conteúdo de arquivos. O agente só pode iniciar e parar serviços, e só com aprovação humana. Reiniciar, pausar, continuar, mudar tipo de início e criar serviço não são expostos ao agente.

## Fluxo de encerramento de processos

1. O agente chama `processo_planejar_encerramento` com os PIDs. O servidor grava um plano em `pasta_planos` e devolve `plano_id`, os alvos e os PIDs excluídos.
2. Se o agente tentar `plano_aplicar` agora, a resposta é **"aguardando aprovação humana"**.
3. Um humano aprova (veja [Aprovação humana](#aprovação-humana)).
4. O agente chama `plano_aplicar` com o `plano_id` e os mesmos PIDs. O servidor:
   - confere o plano (aprovado, não usado, dentro da validade e com os mesmos argumentos);
   - marca o plano como usado **antes** de agir (no máximo uma execução);
   - revalida cada alvo: o PID ainda existe, é o mesmo processo (nome e horário de início) e não virou protegido;
   - encerra com `taskkill /F /T`, só os alvos aprovados e válidos.

## Fluxo de serviços

1. O agente consulta com `servicos_listar` ou `servico_detalhar` e escolhe o serviço.
2. O agente chama `servico_planejar_acao` com `nome` exato e `acao` (`start` ou `stop`). O servidor devolve os passos que executaria. Para `stop`, os dependentes ativos entram antes do alvo.
3. Um humano aprova o plano.
4. O agente chama `servico_aplicar` com o mesmo `plano_id`, `nome` e `acao`. O servidor:
   - confere o plano (aprovado, não usado, válido, argumentos idênticos);
   - marca como usado **antes** de agir;
   - **recalcula o plano do zero** e compara, passo a passo, com o que foi aprovado. Se o estado do serviço ou a cadeia de dependentes mudou, a aplicação é recusada;
   - executa só se o escudo ainda permitir (serviços críticos nunca são parados).
5. O resultado traz o `resultado` (`ok`, `erro`, `recusado` etc.), os passos concluídos e a mensagem.

Em todo caso, o plano expira em `validade_plano_s` segundos (padrão 300). Plano expirado não pode ser aprovado nem aplicado.

## Aprovação humana

Há duas formas, equivalentes. Ambas gravam a aprovação na auditoria (`mcp.aprovar`, com `aprovado_por=tui` ou `aprovado_por=cli`).

**Pela TUI (recomendado):** abra `cli_admin_v3.exe` com o mesmo `config.toml`, aperte `F2`, escolha o plano com as setas e pressione `y` (ou Enter). `Esc` fecha sem aprovar.

**Pela CLI:**

```powershell
C:\local\bin\cli_admin_v3.exe --config C:\local\share\cli_admin_mcp\config.toml mcp pendentes
C:\local\bin\cli_admin_v3.exe --config C:\local\share\cli_admin_mcp\config.toml mcp aprovar <plano_id>
```

Quem aprova é sempre uma pessoa. O servidor MCP não tem ferramenta que marque um plano como aprovado.

## Modelo de ameaças (resumo)

O agente e o texto que ele lê são tratados como não confiáveis. Os principais controles:

| Ameaça | Controle |
|---|---|
| Injeção de instrução por dados (nome de processo, serviço, linha de comando) | Saída JSON estruturada, `sanitizar_campo` (remove controles e caracteres de direção), descrições de ferramenta fixas |
| Vazamento de segredos | Redação de linha de comando (`--password=`, `-p<senha>`, `Bearer`, URL com senha, cadeias de conexão, tokens longos). **Melhor esforço, não garantia** |
| Ação destrutiva indevida | Escrita desligada por padrão; plano com aprovação humana fora do agente; escudos (processos protegidos e serviços críticos) checados no momento de aplicar |
| Parada em cascata inesperada | O plano lista todos os dependentes antes da aprovação, e a aplicação recalcula a cadeia |
| Fuga de escopo no sistema de arquivos | `canonicalize` (resolve `..`, links e junções), raízes permitidas, lista de negação |
| Laço ou abuso do agente | Limites por minuto, por sessão, repetição idêntica, teto de itens e de bytes |
| Elevação de privilégio | Recusa de início como administrador |
| Adulteração do plano entre planejar e aplicar | Hash dos argumentos, uso único, validade, revalidação completa |
| Poluição do canal | stdout exclusivo do protocolo; teste de protocolo confere isso |
| Repúdio | Auditoria de cada chamada, com hash e sem argumentos; aprovações registradas com quem aprovou |

**Limites conhecidos:**

- A redação de segredos cobre formatos comuns. Um segredo em formato inesperado pode passar.
- O MCP não impede o agente de ler o que as ferramentas devolvem; por isso o conteúdo de nomes e linhas de comando é tratado como dado.
- A detecção de execução como administrador usa `net session` e falha de forma silenciosa se o serviço "Servidor" estiver parado.
- Não há transporte HTTP, portanto não há autenticação OAuth nem validação de `Origin`.
- Serviços criados como LocalSystem têm privilégio total. A criação de serviços não é exposta ao MCP por isso.

## Testes

```powershell
cargo test --offline --features mcp
```

Inclui:
- política (redação, caminhos, limites, planos);
- ferramentas com dados falsos, inclusive a listagem de serviços e a regra de `start`/`stop` apenas;
- protocolo por stdio: lista de ferramentas, `allow_apply`, recusa fora da raiz, planejamento de protegido, recusa de parada de serviço crítico, auditoria sem argumentos e recusa elevada;
- fluxo ponta a ponta de **processos**: planeja, recusa a aplicação sem aprovação, aprova com `cli_admin_v3 mcp aprovar`, encerra um processo de teste (`ping`) e recusa o reuso;
- fluxo de **serviços**: planeja a parada do `Spooler`, aprova, aplica, passa pela revalidação e é recusado por falta de Administrador. Sem privilégio, o teste não para nenhum serviço; com privilégio, ele não faz nada.

## Verificação com cliente real

Pendente. Registrar aqui, quando feito: nome e versão do cliente, comando de registro usado, resultado de cada item:

1. Listar ferramentas: só as de leitura e planejamento aparecem; `plano_aplicar` e `servico_aplicar` ausentes com `allow_apply = false`.
2. `processos_listar` com um processo que tenha `--password=` na linha de comando: o valor aparece mascarado.
3. `fs_listar` fora das raízes: recusa. Dentro: lista. `..` e junção não escapam.
4. `servicos_listar` e `servico_detalhar` devolvem os dados sem alterar nada.
5. `servico_planejar_acao` para um serviço crítico (`RpcSs`): recusado, sem plano.
6. Ciclo completo de serviço: planejar, aprovar com F2, aplicar, conferir o estado com `servicos detalhe`.
7. Mais de 60 chamadas em um minuto: a 61ª é recusada.
8. `audit/mcp.log`: uma linha por chamada, com hash e sem argumentos.
9. Iniciar num terminal elevado: recusa com código 2.
