# Brainstorm — funções disruptivas para o CLI_ADMIN_V3

**Data:** 2026-10-17 · **Estado:** brainstorm, nada aprovado nem implementado.
**Premissa do projeto:** Rust, binário único, rápido, sem runtime. Cada ideia abaixo foi filtrada pela escada do Ponytail: *precisa existir? já existe aqui? a stdlib cobre? uma dependência já instalada cobre?* Só depois "construir o mínimo".

## Como ler

- **Esforço:** P (até 1 dia), M (alguns dias), G (semanas).
- **Risco:** o que pode dar errado de verdade (dano à máquina, falso positivo, vazamento, manutenção).
- **Já temos:** peças do V3 ou de outros projetos seus que a ideia reaproveita.
- Nada aqui é medição; esforço e valor são julgamento.

## 1. Ideias ranqueadas

| # | Ideia | Valor | Esforço | Risco | Já temos |
|---|---|---|---|---|---|
| 1 | **Frescor dos backups** (lê o histórico do Backup Unificado e avisa "último backup do job X há N dias, retenção Y") | Alto | P | Baixo | `BACKUP_UNIFICADO` grava `historico.jsonl` com início, status, arquivo e SHA-256; `serde_json` já é dependência |
| 2 | **Quem está usando este arquivo/pasta?** (Restart Manager do Windows) | Alto | M | Baixo | Crate `windows` já em uso; entra como tecla no explorador de arquivos |
| 3 | **Forense de processo**: árvore pai/filho, assinatura digital, SHA-256 do executável, portas abertas do processo, "evidências" de risco | Alto | M | Médio (falso positivo) | Linha de comando completa (V3), `net::scan` (porta ↔ PID), `ProcessShield` |
| 4 | **Linha do tempo**: snapshots periódicos (processos, serviços, portas, inicialização) e "o que mudou desde ontem" | Alto | M | Baixo | `winsys svc-save/svc-diff`, `boot-audit` já existem; falta agendar e comparar |
| 5 | **Servidor MCP** para um agente de IA operar a ferramenta com segurança | Muito alto | G | **Alto** | Domínios em `modules/` já separados de CLI e TUI. **Plano separado** (ver `specs/2026-10-17-mcp-server-design.md`) |
| 6 | **Modo `watch` com alertas** (regras em TOML: CPU > 90% por 60 s, porta caiu, serviço parou, disco < 10%) | Alto | M | Baixo | `tick` em segundo plano, `volume_info`, `net::scan` |
| 7 | **Checagens de endurecimento** (serviço com caminho sem aspas, porta em `0.0.0.0`, itens de inicialização suspeitos) | Médio-alto | M | Médio | `boot-audit`, `winsvc`, `net::scan` |
| 8 | **Treemap de disco** no terminal (retângulos proporcionais, não só barras) | Médio (visual) | M | Baixo | `DiskScan` em segundo plano e `texto::barra` |
| 9 | **Duplicados** (tamanho → hash parcial → hash total, em segundo plano, cancelável) | Médio | M | Médio (apagar errado) | `walkdir`, padrão do `DiskScan`. Exclusão só pela camada de política |
| 10 | **Rede de segurança com VM**: snapshot automático antes de uma ação de risco, com opção de reverter | Médio | P-M | Baixo | `vm snapshot create/revert` já existem |
| 11 | **Saúde do banco** por modelos somente leitura (Oracle: tablespaces e sessões; Postgres: conexões e tamanho) | Médio | P-M | Baixo | Depende da aba Bancos (L3 via cliente externo) |
| 12 | **Sparklines históricos** na aba TOP (últimos 5 minutos de CPU/RAM) | Médio | P | Baixo | `sparkline` já existe na seção Ping |
| 13 | **Paleta de comandos** `:` com busca fuzzy de ações | Médio (UX) | P | Baixo | `fuzzy-matcher` já é dependência |
| 14 | **Relatório HTML único** de suporte (evidência para o cliente) | Médio | M | Baixo | Padrão já usado no CLI_SW_ADM (HTML + Markdown + ZIP) |
| 15 | **Frota / modo servidor** (vários computadores, mTLS) | Alto | G | **Alto** | Nada. Adiar |
| 16 | **Plugins em WebAssembly** | Médio | G | Médio | Nada. Adiar |
| 17 | **Driver SQL próprio** (TNS, TDS) | Baixo | G | Alto | Não fazer: o cliente externo do fabricante já cobre |

## 2. Detalhe das ideias principais

### 1. Frescor dos backups
Lê `<pasta_logs>\historico.jsonl` do Backup Unificado e mostra por job: último status (OK, AVISO, FALHA), idade, tamanho e SHA-256. Alerta quando o último backup OK é mais antigo que o esperado. É a ideia mais barata e liga duas ferramentas suas. **Antes de implementar:** confirmar no `BACKUP_UNIFICADO/backup_unificado/historico.py` os nomes exatos dos campos.

### 2. Quem está usando este arquivo?
API Restart Manager (`RmStartSession`, `RmRegisterResources`, `RmGetList`) devolve os processos que seguram um arquivo. Resolve "não consigo apagar/copiar, está em uso". Entra no explorador de arquivos (tecla `L`) e como ferramenta de leitura no MCP.

### 3. Forense de processo
Evidências, nunca veredito: executável fora de `System32` com nome parecido com o de sistema; sem assinatura válida; em `Temp` ou `AppData`; escutando em `0.0.0.0`. **Lição do CLI_SW_ADM:** mostrar o que foi observado e o nível de confiança, sem afirmar o que não está provado (`licoes/08-processo-de-desenvolvimento.md`, L-PRO-02).

### 4. Linha do tempo
Primeira versão em arquivos JSONL (sem dependência nova). SQLite (`rusqlite`) só se a consulta ficar lenta. Cada snapshot reutiliza as coletas que já existem.

### 5. Servidor MCP
Maior alavanca e maior risco. Dois pontos que mudam o desenho, ambos tratados no plano separado: a linha de comando de um processo pode conter senhas e tokens, e o conteúdo de arquivos e nomes de processo é entrada não confiável para o modelo (injeção de prompt).

## 3. O que não fazer agora (Ponytail)

- **Frota, plugins WASM, driver SQL próprio:** custo alto, sem demanda declarada.
- **Interface gráfica:** a promessa do projeto é terminal rápido.
- **SQLite desde o início:** JSONL cobre linha do tempo e auditoria até que a consulta pese.

## 4. Sequência sugerida

1. Explorador de arquivos e aba Bancos (já em planejamento, com specs e planos).
2. Frescor dos backups (#1) e "quem usa este arquivo" (#2): baixo custo, alto retorno.
3. Servidor MCP somente leitura (plano separado), depois planos com aprovação humana.
4. Forense de processo (#3) e linha do tempo (#4), que também enriquecem o MCP.

## 5. Perguntas em aberto

- **"Agente RLM":** assumi um agente de IA que chama ferramentas, possivelmente em laço longo (modelo recursivo). Isso exige limites de chamadas e respostas pequenas e paginadas. Se "RLM" for outra coisa, o plano do MCP precisa de ajuste.
- **#1 depende de os dois projetos rodarem na mesma máquina** (ou de a pasta de logs ser acessível).
