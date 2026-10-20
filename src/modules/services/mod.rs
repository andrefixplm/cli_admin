//! Gerenciamento de serviços do Windows: leitura detalhada, controle (start, stop,
//! restart, pause, continue), tipo de inicialização e criação. A listagem simples
//! continua em `support::winsvc`, usada pelo snapshot de `winsys`.

pub mod models;
pub mod ops;
pub mod scm;
pub mod shield;

pub use models::{Acao, Espera, Passo, Resolucao, StartType, SvcDetail, Transicao};
pub use ops::{Opcoes, Plano, PassoServico, Relatorio, Resultado};
pub use shield::{Decisao, ServiceShield};
