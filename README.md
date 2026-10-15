# CLI Admin (Rust) — Administrador Modular de VMware Workstation, TOP Métricas e Process Killer

Utilitário de alta performance escrito em **Rust**, desenvolvido para administração do **VMware Workstation**, monitoramento de hardware em tempo real (CPU, RAM, Disco I/O, GPU Engine % e VRAM por processo) e gerenciamento/finalização de processos com navegação por teclado e busca fuzzy.

---

## 🚀 Modos de Operação

A aplicação opera em dois modos distintos:

1. **Modo Interativo TUI (Terminal User Interface):**
   Executar sem argumentos (ou com `-i` / `--interactive`) abre o painel visual com abas, atalhos de teclado e filtros dinâmicos:
   ```bash
   .\target\release\cli_admin.exe
   ```

2. **Modo CLI Não-Interativo (Scriptável):**
   Executar com subcomandos diretos para automação rápida em PowerShell, Batch ou rotinas agendadas:
   ```bash
   # Listar máquinas virtuais
   cli_admin vm list

   # Iniciar máquina virtual em segundo plano
   cli_admin vm start tc-dcta

   # Exibir os 15 processos com maior consumo de VRAM
   cli_admin top -n 15 --sort vram

   # Buscar processos correspondentes e simular finalização
   cli_admin kill chrome --dry-run

   # Exportar relatório de processos com a linha de comando de cada um
   cli_admin export sldworks -o relatorio.csv
   ```

---

## 🎮 Navegação e Atalhos da TUI

| Tecla / Atalho | Ação |
|---|---|
| `1`, `2`, `3` | Alternar diretamente entre as abas (`[1] VMware`, `[2] TOP`, `[3] Processos`) |
| `Tab` / `Shift+Tab` | Percorrer abas para a próxima ou anterior |
| `Setas Cima / Baixo` ou `k / j` | Navegar verticalmente entre os itens da tabela |
| `PageUp` / `PageDown` | Rolar rapidamente 10 itens |
| `?` | Abrir tela com guia de ajuda e atalhos |
| `q` ou `Ctrl+C` | Sair da aplicação |

### Aba 1: 🖥️ VMware Workstation
- `>` ou `s`: Iniciar a máquina virtual selecionada (modo *nogui*)
- `x`: Desligar a máquina virtual selecionada (*soft shutdown*)
- `p`: Pausar a máquina virtual
- `u`: Retomar execução da máquina virtual pausada (*unpause*)
- `z`: Suspender a máquina virtual selecionada
- `n`: Criar novo Snapshot para a máquina virtual
- `a`: Cadastrar uma nova VM na lista (alias + caminho .vmx) — útil para VMs offline (disco externo/rede desconectado), que passam a aparecer com status `OFFLINE` até o caminho voltar a existir
- `r`: Atualizar o inventário e status em tempo real

### Aba 2: 📊 TOP Recursos do Sistema
- `c`: Ordenar processos por **CPU %**
- `m`: Ordenar processos por **Memória RAM**
- `d`: Ordenar processos por **Disco I/O**
- `g`: Ordenar processos por **GPU Engine %**
- `v`: Ordenar processos por **VRAM (MB)**

### Aba 3: 🎯 Gerenciador de Processos (Process Killer)
- `/`: Entrar no modo de busca por digitação (filtro fuzzy instantâneo)
- `Esc` / `Enter`: Concluir digitação e voltar à navegação por setas
- `Ctrl+C`: Limpar o filtro de busca
- `Espaço`: Marcar / desmarcar múltiplos processos simultaneamente
- `k` ou `Delete`: Abrir modal de confirmação para finalizar processo(s)
- `n`: Ordenar processos por **nome** (alfabético)
- `C`: Ordenar processos por **CPU %** (maior primeiro)
- `M`: Ordenar processos por **Memória RAM** (maior primeiro)
- `V`: Ordenar processos por **VRAM** (maior primeiro)
- `e`: Exportar lista filtrada para **CSV** (arquivo com timestamp)
- `E`: Exportar lista filtrada para **JSON** (arquivo com timestamp)
- **Escudo de Proteção:** Processos vitais do Windows (`csrss.exe`, `svchost.exe`, `winlogon.exe`, etc.) são identificados com `🛡️ CRÍTICO` e disparam alertas visuais em vermelho para evitar telas azuis acidentais.
- **Painel de Detalhes:** Ao selecionar um processo, um painel inferior exibe informações detalhadas: path completo, **linha de comando** (a mesma coluna do Gerenciador de Tarefas), usuário, tempo de execução (uptime), PID pai, uso de GPU/VRAM e disco I/O.

---

## 🛠️ Subcomandos CLI

### 1. VMware Workstation (`cli_admin vm`)
```bash
# Listar todas as máquinas virtuais descobertas e status (Running / Stopped / Paused)
cli_admin vm list

# Iniciar VM por alias ou caminho (adicionar --gui para abrir a janela do VMware)
cli_admin vm start tc-dcta
cli_admin vm start "D:\VM\TC_DCTA\Windows_Server_2019.vmx" --gui

# Parar VM (adicionar --hard para corte forçado de energia)
cli_admin vm stop tc-dcta
cli_admin vm stop tc-dcta --hard

# Pausar e retomar
cli_admin vm pause tc-dcta
cli_admin vm unpause tc-dcta

# Suspender e reiniciar
cli_admin vm suspend tc-dcta
cli_admin vm reset tc-dcta

# Gerenciamento de Snapshots
cli_admin vm snapshot list tc-dcta
cli_admin vm snapshot create tc-dcta "Pre_Upgrade_TC"
cli_admin vm snapshot revert tc-dcta "Pre_Upgrade_TC"

# Cadastrar uma nova VM na lista (não precisa existir no momento do cadastro —
# útil para VMs em disco externo/rede que estão temporariamente offline)
cli_admin vm add tc-dcta2 "D:\VM\TC_DCTA2\Windows_Server_2019.vmx" --name "Teamcenter DCTA2"
```

### 2. TOP Métricas (`cli_admin top`)
```bash
# TOP processos por VRAM (padrão)
cli_admin top

# TOP 20 processos ordenados por RAM
cli_admin top -n 20 --sort ram

# TOP 10 processos ordenados por CPU
cli_admin top -n 10 --sort cpu

# TOP 15 processos ordenados por GPU ou Disco I/O
cli_admin top -n 15 --sort gpu
cli_admin top -n 15 --sort io
```

### 3. Process Killer (`cli_admin kill`)
```bash
# Simular busca de processos (não finaliza nada)
cli_admin kill chrome --dry-run

# Buscar e solicitar confirmação interativa antes de matar
cli_admin kill notepad

# Forçar finalização imediata com árvore de filhos sem prompt
cli_admin kill "my_app.exe" --force --tree -y
```

### 4. Relatório de Processos (`cli_admin export`)
Exporta a lista de processos — incluindo a **linha de comando** que iniciou cada um — em CSV ou JSON. O filtro é opcional e usa a mesma busca fuzzy (nome ou PID) da TUI.
```bash
# Todos os processos em CSV (processes_<timestamp>.csv no diretório atual)
cli_admin export

# Apenas processos do SOLIDWORKS, em JSON, com arquivo de saída definido
cli_admin export sldworks --format json --output C:\Temp\sldworks.json
```
Colunas do CSV: `PID, PPID, Nome, CPU%, Memoria_MB, Path, Linha_de_Comando, Usuario, Uptime, Protegido`. O arquivo é gravado em UTF-8 com BOM, para que o Excel exiba acentos corretamente.

> **Observação:** assim como no Gerenciador de Tarefas, a linha de comando de processos de sistema ou de outros usuários só é lida quando o `cli_admin` é executado **como Administrador**; caso contrário, o campo fica vazio e o comando informa quantos processos ficaram sem essa informação.

---

## ⚙️ Arquivo de Configuração (`config.toml`)

O sistema lê o arquivo de configuração na seguinte ordem de precedência:
1. `--config <caminho>`
2. `./config.toml` (diretório atual)
3. `<diretorio_do_exe>/config.toml`
4. `%APPDATA%\cli_admin\config.toml`

### Exemplo:
```toml
[general]
refresh_rate_ms = 1000
log_level = "info"
log_file = "admin.log"

[vmware]
vmrun_path = "C:\\Program Files (x86)\\VMware\\VMware Workstation\\vmrun.exe"
default_start_mode = "nogui"
default_stop_mode = "soft"

scan_directories = [
    "D:\\VM",
    "D:\\VMs",
    "C:\\Users\\andre\\Documents\\Virtual Machines"
]

[[vmware.vms]]
alias = "tc-dcta"
name = "Teamcenter DCTA Windows Server 2019"
path = "D:\\VM\\TC_DCTA\\Windows_Server_2019.vmx"

[metrics]
default_sort = "vram"
top_process_count = 30
enable_gpu_metrics = true
enable_nvml = true

[security]
protected_processes = [
    "csrss.exe",
    "svchost.exe",
    "explorer.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
    "smss.exe",
    "system",
    "registry",
    "dwm.exe"
]
require_kill_confirmation = true
```

---

## 🧪 Testes e Compilação

```bash
# Executar todos os testes automatizados
cargo test

# Compilar versão de desenvolvimento
cargo build

# Compilar binário de produção otimizado
cargo build --release
```
