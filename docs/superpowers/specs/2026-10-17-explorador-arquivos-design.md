# Explorador de arquivos na TUI — Spec de design

**Data:** 2026-10-17 · **Caminho:** arquitetural (subsistema novo) · **Estado:** aguardando revisão do usuário
**Plano de implementação:** `docs/superpowers/plans/2026-10-17-explorador-arquivos.md`
**Pesquisa de base:** `D:\W\DEV\GENERAL_TOOLS\ferramentas_CLI\referencias\gerenciadores-de-arquivos.md`

## 1. Objetivo

Uma aba **[5] Arquivos** na TUI do `cli_admin_v3` para navegar pelo disco, **copiar** arquivos e pastas e **criar pastas**, sem sair do terminal e **sem nunca travar a interface**.

## 2. Escopo

**Dentro (pedido pelo usuário):**
- navegação: entrar na pasta, subir, ir a um caminho digitado, trocar de unidade, mostrar/ocultar ocultos, ordenar (nome, tamanho, data), filtrar por texto;
- seleção múltipla;
- copiar arquivos e pastas (recursivo) para a pasta atual;
- criar pasta.

**Fora (YAGNI agora, ordem sugerida para depois):** mover, renomear, apagar, pré-visualização de conteúdo, busca recursiva, painel duplo, abas de pastas. Mover/renomear/apagar entram depois, **obrigatoriamente** pela camada de política com confirmação (ver §7).

## 3. Abordagens consideradas

| | Descrição | Prós | Contras |
|---|---|---|---|
| **A (recomendada)** | Lista única. `y` marca para cópia, o usuário navega, `p` cola na pasta atual | Menor código; modelo conhecido (ranger, lf, yazi); cabe na tela estreita | Dois passos para copiar |
| B | Painel duplo, `F5` copia de um lado para o outro | Direto para quem usa Total Commander/mc | Dobra a tela e o estado; mais código |
| C | Adotar o crate `tui-file-explorer` | Pronto: busca, ordenação, tema | Exige ratatui ^0.30 e crossterm ^0.29; o projeto usa 0.29 e 0.28, então a atualização afeta a TUI inteira |

**Escolha: A.** Painel duplo (B) pode vir depois sobre o mesmo estado e o mesmo motor de cópia.

## 4. Decisões de design

1. **Nada de I/O na thread da interface.** Listar pasta, calcular plano e copiar rodam em threads de fundo, com resultado consultado a cada tick de 150 ms (`App::poll_background`). Motivo: uma unidade de rede lenta ou desconectada pode demorar minutos, e isso já aconteceu nesta máquina (`volume_info` com `sysinfo::Disks` levou 182 s; `licoes/06-integracao-windows.md`, L-WIN-11).
2. **Resposta fora de ordem é descartada.** Cada listagem tem um número de pedido; só o mais recente vale.
3. **Cópia nunca sobrescreve.** Conflito de nome vira `nome (2).ext`. Isso elimina a pergunta "sobrescrever?" e o risco de perda de dado. Sobrescrever, se vier, é opção explícita futura.
4. **Plano antes de executar.** A cópia calcula um `PlanoCopia` (itens, bytes, renomeados, ignorados), mostra num modal de confirmação e só então executa.
5. **Links e junções não são seguidos.** Qualquer ponto de reanálise (`FILE_ATTRIBUTE_REPARSE_POINT`) é ignorado e listado como "ignorado". Evita laços e cópia de volumes inteiros.
6. **Destino dentro da origem é recusado** (copiar uma pasta para dentro dela mesma geraria recursão infinita).
7. **Espaço livre é conferido** antes de copiar, com `volume_info` (consulta só a unidade do destino).
8. **Cancelável:** `Esc` durante a cópia. Granularidade: entre arquivos. **Limite conhecido:** um arquivo muito grande não é interrompido no meio (`std::fs::copy`). Atualização futura: cópia em blocos.
9. **Auditoria:** toda cópia e criação de pasta grava uma linha em `audit/auditoria.log` (5 campos separados por TAB, sem quebras), no formato do CLI_SW_ADM.
10. **Sem dependência nova.** `std::fs`, `walkdir` (já dependência), `chrono` (já) e a crate `windows` (já) bastam.

## 5. Arquitetura

`app.rs` (1.555 linhas) e `ui.rs` (1.354) já são grandes. **O subsistema novo vive em arquivos próprios**; `app.rs` e `ui.rs` só recebem a ligação.

```
src/audit.rs                    # trilha de auditoria compartilhada (explorador, banco, MCP)
src/modules/fm/
  mod.rs                        # reexporta a API
  names.rs                      # nome_valido, nome_livre
  listing.rs                    # FmEntry, FmSort, listar, ordenar, filtrar
  drives.rs                     # unidades() via GetLogicalDrives (não toca a mídia)
  copy.rs                       # PlanoCopia, planejar, executar, Progresso
src/tui/fm.rs                   # FmState: estado, teclas, threads de fundo
src/tui/fm_ui.rs                # draw_files_tab, modais
```

Dependência entre unidades: `fm_ui` → `fm` (estado) → `modules::fm` (lógica pura) → `std`/`walkdir`/`windows`. A lógica em `modules::fm` não conhece a TUI e é testável com pastas temporárias.

## 6. Interação (teclas da aba)

| Tecla | Ação |
|---|---|
| `↑` `↓` `k` `j`, `PgUp` `PgDn`, `g` `G` | mover o cursor |
| `Enter` `→` `l` | entrar na pasta |
| `←` `h` `Backspace` | subir (na raiz, mostra as unidades) |
| `:` | digitar um caminho |
| `.` | mostrar/ocultar ocultos |
| `s` | alternar ordenação: Nome → Tamanho → Data |
| `/` | filtrar por texto |
| `Espaço` | marcar/desmarcar e descer |
| `y` | guardar o marcado (ou o item do cursor) para copiar |
| `p` | colar na pasta atual (abre a confirmação) |
| `n` | nova pasta (campo de texto) |
| `r` | recarregar |
| `Esc` | cancelar cópia em andamento, ou limpar marcas/filtro |

## 7. Segurança

- **Copiar é não destrutivo** (não sobrescreve, não apaga). O risco restante é escrever em lugar indevido: o modal mostra o destino completo e avisa quando for dentro de `C:\Windows` ou `Program Files`.
- **Nome de pasta validado** (`nome_valido`): sem caracteres inválidos do Windows, sem nomes reservados (`CON`, `NUL`, `COM1`…), sem ponto ou espaço no fim, no máximo 255 caracteres.
- **Operações destrutivas futuras** (mover, apagar): passam por `dry-run` + confirmação + auditoria, e respeitam a lista de proteção, como o `kill` (`licoes/01-seguranca-acoes-destrutivas.md`, L-SEG-01 a L-SEG-07).
- **Permissão negada** em um item não aborta a cópia: o erro entra no relatório final e a cópia continua.

## 8. Tratamento de erros

| Situação | Comportamento |
|---|---|
| Pasta ilegível ou unidade sem resposta | A lista mostra a mensagem de erro; o cursor não se perde; `r` tenta de novo |
| Destino sem espaço | Recusa antes de copiar, informando necessário x livre |
| Falha em um arquivo | Continua os demais; relatório final lista as falhas |
| Cancelamento | Para entre arquivos; informa quantos foram copiados |
| Nome de pasta inválido | Mensagem no próprio modal; não cria |

## 9. Testes

- **Lógica pura** (`modules::fm`): nomes válidos e reservados; `nome_livre`; ordenação (pastas primeiro); ocultos; plano com arquivo, pasta recursiva, conflito, destino dentro da origem, junção ignorada; execução com conteúdo conferido; cancelamento.
- **Estado da TUI**: com o `App` real e teclas simuladas (padrão do teste `v3_disco_so_calcula_por_tecla_sem_travar_e_esc_cancela`): navegar, marcar, `y`/`p`, confirmar, esperar pelo `poll_background`.
- **Tela**: renderização num `TestBackend`, conferindo cabeçalho, linhas, marcas e o modal.
- **Desempenho**: a suíte inteira deve continuar em segundos; um teste que passa a demorar é um sinal a investigar.

## 10. Riscos e perguntas abertas

- **Junções no Windows:** o plano exige um teste real com `mklink /J` antes de confiar na detecção.
- **Caminhos longos (> 260 caracteres):** comportamento do `std::fs` com prefixo `\\?\` a validar no teste.
- **Unidades de rede:** a listagem roda em segundo plano, mas não há como interromper um `read_dir` já bloqueado; a thread fica presa e o resultado é descartado.
- **Aberto:** o painel duplo (B) é desejado já na primeira versão? A recomendação é não.
