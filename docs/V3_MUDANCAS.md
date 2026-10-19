# CLI_ADMIN V3 — o que mudou em relação à V1.0.0

Base: `D:\W\DEV\GENERAL_TOOLS\CLI_ADMIN` (não alterada). Versão: **3.0.0**. Executável: **`cli_admin_v3.exe`**.
Pacote Rust continua `cli_admin` (a biblioteca mantém o nome `cli_admin`); só o binário recebe o nome V3 (`Cargo.toml`, seção `[[bin]]`).

## 1. Linha de comando completa de cada processo

**Problema original:** a lista de processos mostrava só o caminho do executável. A linha completa aparecia apenas no painel de detalhes, e ficava vazia sem administrador.

**Mudanças:**
- Nova coluna **LINHA DE COMANDO** na aba Processos (`src/tui/ui.rs`), com o texto completo que a largura da tela permite. Quando vazia, mostra `— (sem permissão; use Administrador)`.
- Painel de detalhes continua com a linha completa e quebra de linha (`Wrap`).
- Exportação CSV/JSON já trazia a coluna `Linha_de_Comando`; confirmado com teste real (`ping.exe -n 40 127.0.0.1`).
- Teste ponta a ponta `v3_linha_de_comando_completa_do_proprio_processo` (`tests/unit_tests.rs`): cria um `ping`, lê a lista e confere os argumentos.

**Limitação conhecida:** a linha de comando de processos de **outro usuário** ou de **sistema** depende de permissão do Windows. Sem administrador, o campo pode ficar vazio. Processos do próprio usuário saem completos.

## 2. Otimização de coleta

- `SystemMetricsManager` passou a pedir só os campos usados (`src/modules/top/system.rs`, `proc_refresh_kind`). Antes, `ProcessRefreshKind::everything()` lia também `environ`, `cwd` e `root`, a cada tick, sem uso.
- `exe`, `user` e `cmd` usam `UpdateKind::OnlyIfNotSet`: são lidos enquanto estiverem vazios, porque não mudam durante a vida do processo.
- Ganho esperado: menos chamadas ao sistema por tick. **Não medido**; medir antes de afirmar número.

## 3. Correções dos achados do estudo (`ferramentas_CLI/licoes/07-armadilhas-catalogadas.md`)

| ID | Correção na V3 | Onde |
|---|---|---|
| A-01 | Truncamento por caractere (`texto::truncar`), sem panic em nome acentuado | `src/texto.rs`, `src/main.rs` |
| A-02 | Processo protegido **nunca** entra na lista de encerramento, nem com `-y`. Regra em `separar_protegidos` (testada) e revalidada em `execute_kill_pids` (TUI) | `src/modules/process/killer.rs`, `src/main.rs`, `src/tui/app.rs` |
| A-03 | Flags de `kill` e `net kill` agora são opt-out: `--soft` (sem /F) e `--sem-arvore` (sem /T). Padrão continua forçado com árvore | `src/main.rs` |
| A-04 | Falha de VM sai com código `1` (antes saía com `0`) | `src/main.rs`, braços de `VmAction` |
| A-05 | Versão vem do Cargo (`version`, `CARGO_PKG_VERSION`) | `src/main.rs`, `Cargo.toml` |
| A-06 | Encerramento pela TUI revalida o escudo antes do `taskkill` | `src/tui/app.rs` |
| A-07 | Confirmação única (`confirmar`), usada por kill e net kill | `src/main.rs` |
| A-13 | Instruções unificadas em `AGENTS.md` (ver abaixo) | `CLAUDE.md`, `QWEN.md`, `AGENTS.md` |

Não tratados nesta versão: A-08 (bugs herdados do CLI_SW_ADM, que não é este projeto), A-09 (parse do `typeperf`), A-10, A-11 e A-12 (são do CLI_SW_ADM).

## 3.1 Rodada 2 — licenças removidas, Disco sob demanda e gráfico

### Licenças removidas
A ferramenta não gerencia licenças. Saíram:
- aba **Licenças** da TUI (as abas agora são 1 VMware, 2 TOP, 3 Processos, 4 Rede, 5 Suporte);
- comandos `lic list/free/diag/firewall-check/splm-status` e `sw borrow`;
- módulo `support/licensing.rs`, seção `[licensing]` e `solidworks.snl_log_path` do `config.toml` (um `config.toml` antigo com essas chaves continua carregando: chaves desconhecidas são ignoradas);
- seriais no `sw info` e na seção SolidWorks do Suporte (e a função `mask_serial`).

**SolidWorks removido por completo (correção pedida depois):** comando `sw` (`info`, `gpu`, `clean`), seção SolidWorks do Suporte, módulo `support/solidworks.rs`, `[solidworks]` do `config.toml` e o teste de certificação de GPU. As abas e seções restantes não consultam o SolidWorks.

### Disco: só calcula por tecla e sem travar
- **Causa do travamento:** ao trocar para a seção Disco, `refresh_support_section` chamava o cálculo de forma síncrona, na thread da interface. O caminho padrão é `.`, que pode ser uma pasta enorme.
- **Agora:** entrar na seção não calcula nada. `[r]`/`Enter` inicia o cálculo em uma **thread de fundo**; a tela continua navegável, mostra o tempo decorrido, e o resultado chega pelo `tick`. `[Esc]` cancela (a varredura confere o cancelamento a cada entrada visitada).
- Código: `DiskScan` e `start_disk_scan`/`cancel_disk_scan`/`poll_disk_scan` em `src/tui/app.rs`; `disk_top_cancelavel` em `src/modules/support/winsys.rs`. `disk_top` (usado por `winsys disk-top`) segue igual.

### Gráfico de barras no Disco
- Barra de **ocupação da unidade** (usado/total/livre, verde, amarelo a partir de 75% e vermelho a partir de 90%).
- Uma **barra por pasta**, proporcional à maior (`texto::barra`), com cor pela fatia na soma listada (verde, amarelo a partir de 15%, vermelho a partir de 40%), `% LISTA` e `% UNIDADE`.
- A informação da unidade usa `GetDiskFreeSpaceExW` só para a unidade do caminho. **Primeira versão usava `sysinfo::Disks` e levou 182 s nesta máquina** (enumera todas as unidades, incluindo as de rede); foi detectada pelo tempo da suíte de testes e corrigida. A consulta também roda na thread de fundo.

### Verificação desta rodada
- 41 testes passam (9 da biblioteca + 32 em tests/unit_tests.rs), suíte em ~8 s. Novos: cancelamento da varredura, `volume_info`, `barra`, o comportamento da seção Disco pelo `App` real com teclas simuladas (não calcula ao entrar, `r` retorna em menos de 2 s, resultado chega pelo `tick`, `Esc` cancela) e uma renderização da tela em terminal virtual (barras, unidade, abas renumeradas, sem "Licenças").
- **Não verificado:** a TUI num terminal real, com cores e mouse.

### Pendente: outras seções do Suporte também carregam ao entrar
`Boot/Login` (`schtasks`) e `Mapa da LAN` ainda carregam de forma síncrona quando a seção é aberta. Têm o mesmo risco de travar a navegação e podem receber o mesmo tratamento (carregar por tecla, em segundo plano).

## 4. Achados novos (não estavam no catálogo)

- **Busca fuzzy traz processos não relacionados.** `kill ping --dry-run` listou `logioptionsplus_agent.exe`. Com `-y`, isso amplia o alcance do encerramento. Sugestão: busca por nome exato como padrão e fuzzy com `--fuzzy`. **Não corrigido**, porque muda o comportamento do uso atual.
- **Teste de TUI não automatizado.** A TUI compila e a lógica de encerramento tem teste, mas a tela não foi exercitada interativamente nesta sessão.

## 5. Verificação feita

- `cargo build --offline`: compila sem avisos.
- `cargo test --offline`: **40 testes passam** = 7 unitários da biblioteca (3 da base + 4 de `texto`) + 33 em `tests/unit_tests.rs` (29 da base + 4 da V3, incluindo o teste ponta a ponta de linha de comando). Base original: 32.
- Smoke test do binário: `--version` mostra `3.0.0`; `export` traz a linha completa; `kill csrss -y` ignora os 3 processos protegidos; `vm start` inexistente sai com código `1`; `--help` mostra `--soft` e `--sem-arvore`.

## 6. Como executar

```powershell
cd D:\W\DEV\GENERAL_TOOLS\CLI_ADMIN_V3
cargo build --release --offline
.\target\release\cli_admin_v3.exe            # TUI
.\target\release\cli_admin_v3.exe export ping --format csv
```

## 7. Rodada 3 (2026-10-17) — SolidWorks removido e planejamento

### Feito (implementação)
- **SolidWorks removido por completo** (correção pedida): comando `sw`, seção do Suporte, `solidworks.rs`, `[solidworks]` do config e o teste de certificação de GPU.
- **Resultado do disco aparece na hora:** `App::poll_background` é chamado a cada tick de 150 ms em `tui/mod.rs`. Antes, o loop só chamava `tick()` no intervalo das métricas (1 s), atrasando o resultado em até 1 s.

### Planejado (nada disso foi implementado; aguarda aprovação)
| Documento | Assunto |
|---|---|
| `docs/superpowers/brainstorm/2026-10-17-funcoes-disruptivas.md` | 17 ideias ranqueadas por valor, esforço e risco |
| `docs/superpowers/specs/2026-10-17-explorador-arquivos-design.md` e `plans/…-explorador-arquivos.md` | Aba [5] Arquivos: navegar, copiar, criar pasta |
| `docs/superpowers/specs/2026-10-17-aba-banco-de-dados-design.md` e `plans/…-aba-banco-de-dados.md` | Aba [6] Bancos, pasta `tools\db_clients\`, comando `db` |
| `docs/superpowers/specs/2026-10-17-mcp-server-design.md` e `plans/…-mcp-server.md` | Servidor MCP separado, com modelo de ameaças |

### Validação do código dos planos (projetos descartáveis, fora do V3)
- Explorador, Tarefas 1–6: 26 testes passam. MCP, Tarefas 1–5: 36 testes passam. Aba Bancos, Tarefas 2–8: 39 testes passam, após corrigir 4 defeitos encontrados na validação.
- **Não validado:** as partes da TUI (explorador T7–9, Bancos T9–10), o servidor `rmcp` e a configuração real.

### Decisões que dependem do usuário
1. **"SPDB":** qual ferramenta é? (o plano a trata como entrada de configuração).
2. Explorador: modelo A (marcar com `y`, colar com `p`) está bom, ou prefere painel duplo?
3. MCP: revisão-alvo 2026-07-28 (a do SDK); raízes de arquivo que o agente pode ver; escrita (nível A) agora ou depois; autorização para o `cargo` baixar o `rmcp` (não está no cache).
4. "Agente RLM": confirma a premissa de um agente de IA que chama ferramentas em laço?
