# Aba Bancos (teste de conexão e ferramentas de cliente) — Spec de design

**Data:** 2026-10-17 · **Caminho:** arquitetural (subsistema novo) · **Estado:** aguardando revisão do usuário
**Plano de implementação:** `docs/superpowers/plans/2026-10-17-aba-banco-de-dados.md`

## 1. Objetivo

Uma aba **[6] Bancos** (Suporte passa a **[7]**) e o subcomando `db` para **testar a conexão** com Oracle, PostgreSQL e SQL Server e **abrir os clientes** instalados em uma pasta de ferramentas dentro do projeto, no estilo do que o DBeaver faz com seus drivers, mas mantendo a ferramenta pequena e rápida em Rust.

## 2. Perguntas em aberto (precisam de resposta antes do plano)

1. **"SPDB":** não identifiquei essa ferramenta. O plano trata cada ferramenta como uma **entrada de configuração** (nome, executável, tipo, GUI ou console), então qualquer uma entra sem alterar código. Candidatos que imagino: *SQL Developer* (GUI da Oracle) ou *SQLcl* (CLI da Oracle). **Qual é?**
2. **Quem coloca os executáveis na pasta?** Este spec assume que **o usuário** os copia (ver §6, licenças).
3. **Qual cliente Oracle?** Instant Client com o pacote SQL*Plus, ou cliente completo? Muda os nomes de arquivo a descobrir.

## 3. Como o DBeaver faz, e o que muda aqui

- O DBeaver usa **drivers JDBC** (bibliotecas Java `.jar`), a maioria **incluída ou baixada automaticamente**, e um **Driver Manager** para adicionar o que faltar (página oficial de drivers do DBeaver).
- Esta ferramenta não tem JVM nem JDBC. Ela **usa os clientes de linha de comando e gráficos do fabricante como processos externos** e faz sozinha apenas os testes de rede, que são pequenos.

| | DBeaver | CLI_ADMIN_V3 |
|---|---|---|
| Acesso ao banco | Driver JDBC embutido | Cliente do fabricante (`sqlplus`, `psql`, `sqlcmd`) em processo externo |
| Instalação do driver | Automática ou pelo Driver Manager | Pasta `tools\db_clients\` preenchida pelo usuário; a aba só **descobre** o que há |
| Interface | Gráfica própria | TUI + abre as ferramentas gráficas dos fabricantes |

## 4. Abordagens consideradas

| | Descrição | Prós | Contras |
|---|---|---|---|
| **A (recomendada)** | Teste em 3 níveis: TCP e protocolo feitos em Rust; login e `SELECT 1` pelo cliente externo da pasta | Sem dependência nova; sem driver para manter; usa a ferramenta oficial | Depende de o cliente estar na pasta |
| B | Drivers nativos em Rust (`tokio-postgres`, `tiberius`, crate `oracle`) | Sem processo externo | Três dependências grandes; o `oracle` ainda exige as bibliotecas do cliente Oracle; cresce o binário |
| C | Só abrir os clientes, sem testar | Mínimo | Não atende "testes de conexão" |

**Escolha: A.** Um driver nativo (B) só entra se um teste mostrar que o cliente externo é lento demais para o uso.

## 5. Os três níveis de teste

| Nível | O que prova | Como | Mostra |
|---|---|---|---|
| **1 — TCP** | A porta responde | Conexão TCP com timeout (reaproveita `support::probe_port`) | OK/FALHA + tempo em ms |
| **2 — Protocolo** | Do outro lado há mesmo um banco desse tipo | PostgreSQL: pedido `SSLRequest` (8 bytes), resposta `S` ou `N`. SQL Server e Oracle: **fase 2** (pacote pré-login TDS e pacote de conexão TNS); até lá, esse nível fica "não disponível" para eles | OK/FALHA/N/D |
| **3 — Login** | Usuário, senha e serviço funcionam | Cliente externo com uma consulta fixa de versão | Versão do banco ou o erro `ORA-`/`FATAL` |

**Por que o nível 2 só cobre PostgreSQL agora:** é o único trivial e verificável sem credenciais. Para Oracle, o `tnsping` pode cumprir o papel se vier no pacote do cliente; a disponibilidade do `tnsping` no Instant Client **não está confirmada** e será verificada no plano.

## 6. Pasta de ferramentas e licenças

```
tools/
  README.md                      # explica onde colocar cada coisa (versionado)
  db_clients/                    # conteúdo ignorado pelo Git
    oracle/19c/sqlplus.exe  (+ DLLs do Instant Client)
    oracle/sqlcl/bin/sql.exe
    postgres/16/bin/psql.exe
    postgres/pgadmin/pgAdmin4.exe
    sqlserver/18/sqlcmd.exe
```

- **A ferramenta nunca baixa nem redistribui binários de fabricante.** Motivo (verificado na pesquisa):
  - O **Instant Client da Oracle** permite redistribuição **somente** se o usuário final aceitar termos "materialmente consistentes" (licença OTN). Se o **SQL\*Plus** pode ser redistribuído nas mesmas condições **não ficou confirmado**; é preciso conferir a licença da Oracle antes de qualquer distribuição.
  - O **pgAdmin** é distribuído sob a *PostgreSQL License* (permissiva, parecida com BSD ou MIT). No desktop, é um programa de bandeja que sobe um servidor Python e abre o navegador, ou seja, **ferramenta gráfica**, não um cliente de linha de comando. O cliente de linha de comando do PostgreSQL é o `psql`.
- Portanto o usuário copia o que já possui. A aba **descobre** o que encontrar, sem acessar a internet.
- O `.gitignore` do projeto ignora `tools/db_clients/**` e mantém só `tools/README.md`.

### Regras de descoberta

Varredura de `tools/db_clients` até 4 níveis, comparando o nome do arquivo (sem diferenciar maiúsculas) com uma tabela:

| Arquivo | Tipo | Modo |
|---|---|---|
| `sqlplus.exe` | Oracle | console |
| `sql.exe` (SQLcl) | Oracle | console |
| `psql.exe` | PostgreSQL | console |
| `sqlcmd.exe` | SQL Server | console |
| `pgAdmin4.exe` | PostgreSQL | gráfica |
| `sqldeveloper.exe` | Oracle | gráfica |
| entradas extras de `[[database.tools]]` no `config.toml` | qualquer | conforme a configuração (é aqui que o "SPDB" entra) |

A **versão** vem do nome da pasta mais próxima que pareça versão (`19c`, `16`, `18`); sem isso, "—". Não se executa o binário para descobrir versão (custa tempo e pode abrir janela).

## 7. Configuração

```toml
[database]
tools_dir = "tools/db_clients"   # relativa à pasta do executável; pode ser absoluta
timeout_ms = 8000

[[database.instances]]
name = "TC_ORACLE"
kind = "oracle"                  # oracle | postgres | sqlserver
host = "srv-oracle01"
port = 1521
service = "ORCLPDB"              # Oracle: service name do PDB (não SID); outros: nome do banco
user = "infodba"
password_env = "TC_ORACLE_PWD"   # NOME da variável de ambiente. A senha não fica no arquivo
tool = "oracle/19c"              # opcional: qual cliente usar

[[database.tools]]               # opcional: ferramenta fora da tabela padrão
id = "spdb"
label = "SPDB"
kind = "oracle"
exe = "spdb/spdb.exe"
gui = true
```

## 8. Segurança

- **Senha só por variável de ambiente** (`password_env`), lida na hora do teste. Nunca em `config.toml`, em argumentos de linha de comando, em log ou em tela.
- **Como a senha chega ao cliente**, na mesma linha do que o `APLICACAO_BD` já adota (senha fora do argv):
  - Oracle `sqlplus`: pela **entrada padrão** (stdin).
  - PostgreSQL `psql`: variável `PGPASSWORD` **só no processo filho**.
  - SQL Server `sqlcmd`: variável `SQLCMDPASSWORD` **só no processo filho**.
  - Risco residual: o ambiente de um processo filho é legível por quem tem o mesmo usuário ou administrador.
- **Consultas fixas** (versão do banco). A fase 1 **não executa SQL digitado**, o que elimina injeção de SQL pela ferramenta.
- **Tempo limite** em todo processo filho; ao estourar, o processo é encerrado.
- **Saída truncada** (2 KB) e com a senha mascarada, caso o cliente a repita num erro.
- **Abrir cliente no console:** a senha não é passada; o usuário digita no próprio cliente.
- Uma instância sem `password_env` definida ou com a variável ausente só roda os níveis 1 e 2, e informa o motivo.

## 9. Arquitetura

```
src/modules/database/
  mod.rs        # reexporta
  model.rs      # DbKind, Etapa, ResultadoTeste
  config.rs     # (os structs ficam em src/config.rs; aqui só funções de resolução de caminho)
  tools.rs      # descobrir(tools_dir, extras) -> Vec<Ferramenta>
  probe.rs      # nível 1 e nível 2
  client.rs     # nível 3: montar_comando (pura) + executar com timeout
src/tui/db.rs      # DbState: estado, teclas, testes em segundo plano
src/tui/db_ui.rs   # draw_db_tab
```

O `src/config.rs` ganha `DatabaseConfig`, `DbInstance` e `DbToolEntry`. Os testes de nível 3 rodam em `tokio::spawn` e devolvem por canal, para a tela nunca esperar.

## 10. Interação

Tela dividida: **instâncias** (com os indicadores `L1 L2 L3` e a latência), **detalhe** do último teste e **ferramentas** encontradas.

| Tecla | Ação |
|---|---|
| `↑` `↓` | escolher instância |
| `t` | testar a instância (níveis 1 a 3) |
| `T` | testar todas |
| `Enter` | abrir o cliente de console da instância, em nova janela |
| `g` | abrir a ferramenta gráfica compatível |
| `r` | reescanear a pasta de ferramentas |
| `Tab` | alternar foco entre instâncias e ferramentas |

CLI equivalente: `db list`, `db tools`, `db test <nome> [--nivel 1|2|3]`, com código de saída `0` (todos os níveis pedidos OK) e `1` (falha).

## 11. Testes

- **Descoberta** com árvore de pastas temporária.
- **Nível 1:** contra um `TcpListener` local aberto e contra uma porta fechada.
- **Nível 2:** servidor falso que responde `S`, `N` e lixo; confere o pedido de 8 bytes.
- **Nível 3:** `montar_comando` é função pura; o teste afirma que a senha **não** aparece nos argumentos. A execução usa `cmd.exe` como cliente de mentira, para testar timeout, truncamento e mascaramento.
- **Tela:** `TestBackend` com instâncias e ferramentas fictícias.
- **Não testado automaticamente:** login real em Oracle, PostgreSQL ou SQL Server. Exige um banco de teste e fica como verificação manual registrada no STATUS.

## 12. Riscos

- Nome e disponibilidade dos executáveis variam por versão do cliente; a tabela de descoberta é ajustável por configuração.
- `sqlplus` lendo a senha pela entrada padrão depende da versão; confirmar com o cliente real antes de dar o nível 3 por pronto.
- Se o SPDB for uma ferramenta que exige argumentos próprios, o campo `args` por ferramenta pode ser necessário.
