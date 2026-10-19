# Definições do Projeto — CLI Admin V3

## Objetivo

Dar a um administrador Windows, num único executável, o controle de máquinas virtuais do VMware Workstation, a visão de consumo do sistema, o encerramento seguro de processos e portas, a cópia de arquivos e o diagnóstico de rede e de suporte. A TUI é o uso principal; os subcomandos servem para scripts.

Desde a V3, um **servidor MCP** permite que um agente de IA consulte o sistema. O agente nunca encerra nada sozinho: encerramentos passam por um plano aprovado por um humano.

## Escopo

**Dentro:**

- VMware: ligar, parar, pausar, suspender, snapshots, cadastro de VMs.
- Processos: listar, buscar, encerrar (com escudo de protegidos), exportar.
- Portas: portas em escuta ↔ processo, encerramento do dono.
- Explorador: navegar, marcar, copiar arquivos e pastas, criar pasta.
- Diagnóstico: ping, probe de porta, mapa da LAN, throughput, DNS.
- Suporte: disco sob demanda, snapshot de serviços, boot, notas.
- MCP: leitura de processos, portas e pastas; planejamento e aplicação de encerramento com aprovação humana.

**Fora (por decisão):**

- Licenças FlexNet/SNL e SolidWorks: removidos da V3.
- Mover, renomear e apagar arquivos: adiados. Quando entrarem, passam pela mesma política de plano e confirmação.
- Ler conteúdo de arquivos pelo MCP.
- Ferramenta do MCP que executa comando arbitrário. Não existe e não está prevista.
- Transporte HTTP do MCP: adiado.

## Arquitetura

- **Um executável principal** (`cli_admin_v3`) com TUI e CLI. Depende só de crates comuns.
- **Um executável MCP separado** (`cli_admin_mcp`), atrás da feature `mcp`. Assim a superfície de rede e o `rmcp` não entram no executável do dia a dia.
- **Lógica fora da interface:** os módulos em `src/modules/` não conhecem a TUI nem a CLI. A TUI e o MCP os chamam.
- **Política pura do MCP:** `src/mcp/policy.rs`, `approval.rs` e `tools.rs` não dependem do SDK. Só `server.rs` e o binário dependem do `rmcp`.

## Decisões de arquitetura

1. **Nada de E/S de disco ou rede na thread da interface.** Listagens, cálculos e cópias rodam em threads de fundo e são consultados a cada tick (150 ms). Motivo: uma unidade de rede lenta travou a TUI nesta máquina (`volume_info` levou 182 s).
2. **Escudo de processos protegidos (`ProcessShield`).** Processos do núcleo do Windows e os listados em `security.protected_processes` nunca entram numa lista de encerramento, nem com `-y`, nem na TUI, nem pelo MCP. A revalidação é feita no momento do encerramento.
3. **Cópia nunca sobrescreve.** Conflito vira `nome (2).ext`. Links e junções não são seguidos. A cópia mostra o plano antes e confere espaço livre.
4. **MCP: aprovação fora do canal do agente.** O agente cria o plano; um humano aprova num terminal com `cli_admin_v3 mcp aprovar`. O agente não tem como aprovar. O plano expira em 5 minutos (padrão), é de uso único e exige os mesmos PIDs ao aplicar.
5. **MCP: segurança por padrão.** Escrita desligada (`allow_apply = false`), servidor recusa iniciar como administrador, descrições de ferramenta fixas, saídas estruturadas e sanitizadas, limites de taxa, tamanho e repetição.
6. **MCP: redação de segredos é melhor esforço.** Não é garantia. Está documentada como tal em `MCP.md`.
7. **Dependência nova só quando necessária.** O `rmcp` é a única dependência direta da V3 que não estava antes, e só entra com a feature `mcp`.

## Domínio

- **VM:** arquivo `.vmx` do VMware, com alias e nome amigável opcionais. Estados: Running, Stopped, Offline (caminho inacessível).
- **Processo:** PID, nome, CPU, memória, usuário, caminho, linha de comando, protegido ou não.
- **Plano (MCP):** alvos (PID, nome, horário de início), validade, marcas de aprovação e de uso. Armazenado em arquivo.
- **Auditoria:** uma linha por evento, com cinco campos separados por TAB (quando, usuário ou sessão, ação, resultado, mensagem). Nunca contém argumentos do MCP, só o hash deles.

## Convenções

- Código e comentários em **português (pt-BR)**, com acentos.
- Mensagens de erro e saída da TUI em pt-BR.
- Testes em `tests/` para a API pública e testes unitários na própria fonte para lógica interna. Lógica pura é testada sem disco, rede nem Windows quando possível.
- Commits e pull requests só quando pedidos.
- Documentação em `docs/`, com estrutura `STATUS.md`, `README.md` e `Definicoes_do_Projeto.md`, mais `MCP.md`.

## Integrações externas

| Integração | Uso | Observação |
|---|---|---|
| VMware Workstation (`vmrun.exe`) | Operações de VM | Caminho configurável em `[vmware]` |
| `taskkill` | Encerrar processos | Usado pela CLI, pela TUI e pelo MCP |
| `typeperf` | GPU por processo (contadores PDH) | Cache em segundo plano; formato do CSV é frágil |
| `nvidia-smi` | GPU e VRAM da NVIDIA | Opcional; sem driver, retorna nada |
| `net session` | Detectar execução como administrador | Depende do serviço "Servidor" |
| `ping`, `schtasks`, `ntttcp` | Diagnóstico e suporte | Algumas ações exigem as ferramentas instaladas |
| IP Helper / APIs do Windows | Portas, disco, volumes | Crate `windows` |
| `rmcp 3.5.1` (feature `mcp`) | SDK do Model Context Protocol | Revisão de protocolo 2026-07-28 |

## Pontos em aberto

- Aba Bancos e as ferramentas de banco do MCP (plano pronto, código não iniciado).
- Ferramenta `rede_probe` do MCP e a chave `hosts_permitidos`.
- Revalidar a interface da TUI num terminal real.
