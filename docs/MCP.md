# Servidor MCP — `cli_admin_mcp`

Servidor do **Model Context Protocol** (revisão **2026-07-28**) que expõe ao agente de IA um subconjunto seguro do `cli_admin_v3`. Transporte: **stdio** (o cliente inicia o processo). Não há porta de rede.

## Compilar

```powershell
cargo build --release --offline --features mcp
# -> target\release\cli_admin_mcp.exe
```

Sem `--features mcp`, o binário não é compilado e o `cli_admin_v3.exe` não muda.

## Executar

```powershell
.\target\release\cli_admin_mcp.exe --config C:\caminho\config.toml
```

| Opção | Efeito |
|---|---|
| `--config <arquivo>` | Usa esta configuração (mesma precedência do app) |
| `--permitir-elevado` | Permite iniciar como administrador. **Sem essa opção, o servidor recusa** e sai com código `2` |

Regras de execução:

- **stdout é do protocolo.** Nenhuma linha de log vai para o stdout. Logs vão para o arquivo de `general.log_file`.
- Iniciar **a partir de um terminal elevado é recusado**. Use um terminal comum.

## Registrar no opencode

Cliente em uso: **opencode 1.18.34**. A configuração global fica em `%USERPROFILE%\.config\opencode\opencode.jsonc`:

```jsonc
"mcp": {
  "cli_admin": {
    "type": "local",
    "command": ["C:\\Users\\andre\\.local\\bin\\cli_admin_mcp.exe", "--config", "C:\\Users\\andre\\.local\\share\\cli_admin_mcp\\config.toml"],
    "enabled": true,
    "timeout": 15000
  }
}
```

Verificar: `opencode mcp list` deve mostrar `cli_admin connected`.

**Atenção:** o servidor recusa iniciar num terminal elevado. Use um terminal comum (não administrador) para executar o `opencode`. Se a sessão estiver elevada, `opencode mcp list` mostra `cli_admin failed` com `Connection closed`.

Para outros clientes, o formato muda. Em geral basta indicar o comando e os argumentos do servidor stdio. Confira a documentação do cliente escolhido.

## Ferramentas

| Ferramenta | Nível | Descrição |
|---|---|---|
| `processos_listar` | Leitura | Processos com CPU, memória, usuário, caminho e linha de comando redigida. `filtro`, `ordenar` (`cpu`, `ram`, `nome`), `limite` (até 200), `offset` |
| `portas_listar` | Leitura | Portas TCP/UDP e o processo dono. `so_escuta`, `filtro` (nome ou porta), `limite`, `offset` |
| `fs_listar` | Leitura | Nomes e tamanhos de uma pasta **dentro das raízes permitidas**. Não lê conteúdo |
| `processo_planejar_encerramento` | Planejamento | Recebe até 50 PIDs e cria um plano. Protegidos e inexistentes ficam de fora. Devolve `plano_id` |
| `plano_aplicar` | Aplicação | Só existe com `allow_apply = true`. Recebe `plano_id` e **os mesmos PIDs**. Só executa plano aprovado, não usado e dentro da validade |

Não existe ferramenta para executar comando livre, SQL ou ler arquivos.

## Fluxo de encerramento com aprovação

1. O agente chama `processo_planejar_encerramento` com os PIDs. O servidor grava um plano em `pasta_planos` e devolve o `plano_id`, os alvos e os PIDs excluídos.
2. Se o agente tentar `plano_aplicar` agora, a resposta é **"aguardando aprovação humana"**.
3. Um humano, em outro terminal, revisa e aprova:

   ```powershell
   .\cli_admin_v3.exe --config C:\caminho\config.toml mcp pendentes
   .\cli_admin_v3.exe --config C:\caminho\config.toml mcp aprovar <plano_id>
   ```

4. O agente chama `plano_aplicar` com o `plano_id` e os mesmos PIDs. O servidor:
   - confere o plano (aprovado, não usado, dentro da validade e com os mesmos argumentos);
   - marca o plano como usado **antes** de agir (no máximo uma execução);
   - revalida cada alvo: o PID ainda existe, é o mesmo processo (nome e horário de início) e não virou protegido;
   - encerra com `taskkill /F /T`, só os alvos aprovados e válidos.
5. Toda tentativa, inclusive recusada, grava uma linha em `audit/mcp.log`.

O plano expira em `validade_plano_s` segundos (padrão 300). Plano expirado não pode ser aprovado nem aplicado.

## Configuração `[mcp]`

| Chave | Padrão | Efeito |
|---|---|---|
| `allow_apply` | `false` | Liga `plano_aplicar`. Desligado, a ferramenta nem aparece |
| `expor_linha_de_comando` | `true` | `false` omite o campo em `processos_listar` |
| `raizes_permitidas` | pasta do perfil do usuário | Pastas que `fs_listar` pode listar. Lista vazia = recusa tudo |
| `caminhos_negados` | `.ssh`, `.aws`, `.azure`, `.gnupg`, `.kube`, `.docker`, `.git-credentials`, `*.pem`, `*.pfx`, `*.key`, `.env`, `NTUSER.DAT`, `Login Data`, `Cookies`, `Credentials` | Componentes ou padrões bloqueados em qualquer ponto do caminho |
| `limite_por_minuto` | `60` | Chamadas por minuto na sessão |
| `limite_sessao` | `1000` | Chamadas totais na sessão |
| `max_resposta_bytes` | `65536` | Teto de cada resposta |
| `max_campo` | `512` | Tamanho máximo de cada campo de texto vindo do sistema |
| `validade_plano_s` | `300` | Validade do plano |
| `pasta_planos` | `mcp/pendentes` | Onde os planos ficam. **Use caminho absoluto** |
| `auditoria` | `audit/mcp.log` | Arquivo de auditoria. **Use caminho absoluto** |

> Chaves reservadas, **sem efeito ainda**: `db_allow_login` e `hosts_permitidos`. As ferramentas que as usariam ainda não existem.

Caminhos relativos são resolvidos a partir do diretório atual. O servidor e o `mcp aprovar` precisam enxergar a **mesma** pasta de planos, por isso use caminhos absolutos.

## Modelo de ameaças (resumo)

O agente e o texto que ele lê são tratados como não confiáveis. Os principais controles:

| Ameaça | Controle |
|---|---|
| Injeção de instrução por dados (nome de processo, linha de comando) | Saída JSON estruturada, `sanitizar_campo` (remove controles e caracteres de direção), descrições de ferramenta fixas |
| Vazamento de segredos | Redação de linha de comando (`--password=`, `-p<senha>`, `Bearer`, URL com senha, cadeias de conexão, tokens longos). **Melhor esforço, não garantia** |
| Ação destrutiva indevida | Escrita desligada por padrão; plano com aprovação humana fora do agente; escudo de protegidos no momento de aplicar |
| Fuga de escopo no sistema de arquivos | `canonicalize` (resolve `..`, links e junções), raízes permitidas, lista de negação |
| Laço ou abuso do agente | Limites por minuto, por sessão, repetição idêntica, teto de itens e de bytes |
| Elevação de privilégio | Recusa de início como administrador |
| Adulteração do plano entre planejar e aplicar | Hash dos argumentos, uso único, validade, mesmos PIDs exigidos |
| Poluição do canal | stdout exclusivo do protocolo; teste de protocolo confere isso |
| Repúdio | Auditoria de cada chamada, com hash e sem argumentos |

**Limites conhecidos:**

- A redação de segredos cobre formatos comuns. Um segredo em formato inesperado pode passar.
- O MCP não bloqueia o agente de ler o que as ferramentas devolvem; por isso o conteúdo de nomes e linhas de comando é tratado como dado.
- A detecção de execução como administrador usa `net session` e falha de forma silenciosa se o serviço "Servidor" estiver parado.
- Não há transporte HTTP, portanto não há autenticação OAuth nem validação de `Origin`. Isso só passa a valer se o transporte HTTP for implementado.

## Testes

```powershell
cargo test --offline --features mcp
```

Inclui: política (redação, caminhos, limites, planos), ferramentas com dados falsos, protocolo por stdio (lista de ferramentas, `allow_apply`, recusa fora da raiz, planejamento de protegido, auditoria sem argumentos, recusa elevada) e um **fluxo ponta a ponta** que planeja, recusa a aplicação sem aprovação, aprova com `cli_admin_v3 mcp aprovar`, encerra um processo de teste (`ping`) e recusa o reuso.

## Verificação com cliente real

Pendente. Registrar aqui, quando feito: nome e versão do cliente, comando de registro usado, resultado de cada item:

1. Listar ferramentas: só as de leitura e planejamento aparecem; `plano_aplicar` ausente com `allow_apply = false`.
2. `processos_listar` com um processo que tenha `--password=` na linha de comando: o valor aparece mascarado.
3. `fs_listar` fora das raízes: recusa. Dentro: lista. `..` e junção não escapam.
4. Mais de 60 chamadas em um minuto: a 61ª é recusada.
5. `audit/mcp.log`: uma linha por chamada, com hash e sem argumentos.
6. Iniciar num terminal elevado: recusa com código 2.
