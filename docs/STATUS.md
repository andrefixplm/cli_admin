# STATUS — CLI Admin V3

**Versão:** 3.0.0 (`cli_admin_v3.exe`; servidor MCP `cli_admin_mcp.exe`, feature `mcp`)
**Branch:** `feature/cmdline-export` · último commit enviado: `38b7088`
**Atualizado em:** 2026-10-19

## Estado geral

- `cargo build --offline` e `cargo test --offline` passam, sem avisos.
- **126 testes** sem a feature `mcp` e **133** com ela (`cargo test --offline --features mcp`).
- Plataforma: somente Windows (APIs do Windows, `vmrun.exe`, `taskkill`, `typeperf`, `net session`).

## Funcionando

| Área | Situação |
|---|---|
| VMware (aba 1, `vm`) | Listar, ligar, parar, pausar, suspender, snapshots, cadastrar VM |
| TOP (aba 2, `top`) | CPU, RAM, disco, GPU e VRAM por processo |
| Processos (aba 3, `kill`, `export`) | Busca fuzzy, seleção múltipla, encerramento com escudo de processos protegidos, exportação CSV/JSON com linha de comando |
| Rede (aba 4, `net`, `netdiag`) | Portas ↔ processo, encerramento do dono da porta, ping, probe, mapa da LAN, throughput, DNS |
| Arquivos (aba 5) | Navegação, marcação, copiar arquivos e pastas (com plano, sem sobrescrita, cancelável), criar pasta. Ações gravadas em `audit/auditoria.log` |
| Suporte (aba 6, `winsys`, `notes`) | Disco sob demanda com cancelamento, snapshot de serviços, boot, notas em Markdown |
| Servidor MCP (`cli_admin_mcp`) | Ferramentas de leitura, planejamento de encerramento e aplicação com aprovação humana. Ver `MCP.md` |

## Parcial ou não verificado

- **TUI em terminal real:** a lógica e a renderização em terminal virtual são testadas. Interação com teclas, cores e mouse não foram verificadas manualmente.
- **Servidor MCP com cliente real:** o protocolo é testado por stdio, mas nenhum cliente MCP foi registrado e usado de ponta a ponta.
- **Compatibilidade MCP:** só a revisão 2026-07-28 é testada. Clientes que negociam 2025-11-25 por `initialize` não foram testados.
- **Elevação:** o servidor MCP recusa iniciar como administrador. A detecção usa `net session` e falha de forma silenciosa se o serviço "Servidor" estiver parado.

## Não implementado (pendente)

- Aba **Bancos** e as ferramentas `db_instancias` / `db_testar` do MCP. O plano existe em `docs/superpowers/plans/2026-10-17-aba-banco-de-dados.md`.
- Ferramenta `rede_probe` do MCP. A chave `hosts_permitidos` existe na configuração, mas ainda não tem efeito.
- Transporte HTTP do MCP (fase M4 do spec). Não planejado para agora.
- Mover, renomear e apagar arquivos no explorador (fora do escopo da primeira versão).

## Documentação e repositório

- `AGENTS.md` (raiz) está desatualizado: descreve 3 abas e não cita o explorador, o MCP nem as abas 4–6 atuais.
- `docs/V3_MUDANCAS.md` tem contagens de testes antigas (“41” e “40”). O histórico de mudanças continua válido.
- Mudanças do explorador (`38b7088`) já estão no remoto. As mudanças do MCP, o `.gitignore` e o `Cargo.toml` (feature `mcp`) estão **sem commit**.
- `target/` deixou de aparecer como não rastreado por causa do novo `.gitignore`.

## Ambiente

- Disco `C:` estava em 100% durante a sessão (cerca de 1,9 GB livres). Builds completos com `debug` ocupam vários GB. Recomendado builds com `CARGO_PROFILE_DEV_DEBUG=0` ou em diretório fora do `C:`.
- A sessão do Claude Code usada no desenvolvimento estava elevada. Por isso, o servidor MCP só inicia com `--permitir-elevado` nesse ambiente.

## Próximos passos sugeridos

1. Commit da etapa MCP (sem incluir `target/`).
2. Registrar o servidor num cliente MCP real e fazer a verificação manual descrita em `MCP.md`.
3. Atualizar `AGENTS.md` e `V3_MUDANCAS.md` com o estado atual.
4. Decidir o plano da aba Bancos antes de implementar `db_*` no MCP.
