use cli_admin::config::McpConfig;
use cli_admin::mcp::tools::{fs_listar, portas_listar};
use cli_admin::modules::net::{NetConnection, NetProto, TcpState};

fn conexao(pid: u32, nome: &str, porta: u16, estado: Option<TcpState>) -> NetConnection {
    NetConnection {
        proto: NetProto::Tcp,
        ipv6: false,
        local_ip: "0.0.0.0".into(),
        local_port: porta,
        remote_ip: "0.0.0.0".into(),
        remote_port: None,
        state: estado,
        pid,
        process_name: nome.into(),
        is_protected: false,
    }
}

#[test]
fn portas_pagina_filtra_e_so_escuta() {
    let conns = vec![
        conexao(1, "srv.exe", 8080, Some(TcpState::Listen)),
        conexao(2, "cliente.exe", 50000, Some(TcpState::Established)),
        conexao(3, "SRV2.exe", 9000, Some(TcpState::Listen)),
    ];
    let v = portas_listar(&conns, true, None, 50, 0, &McpConfig::default());
    assert_eq!(v["total"], 2, "só LISTEN");
    let v = portas_listar(&conns, false, Some("srv"), 50, 0, &McpConfig::default());
    assert_eq!(v["total"], 2, "nome contém 'srv', sem diferenciar caixa");
    let v = portas_listar(&conns, false, Some("50000"), 50, 0, &McpConfig::default());
    assert_eq!(v["itens"][0]["pid"], 2, "filtro por porta");
}

#[test]
fn portas_limita_a_200() {
    let conns: Vec<NetConnection> = (0..300).map(|i| conexao(i, "p.exe", 1000, None)).collect();
    let v = portas_listar(&conns, false, None, 10_000, 0, &McpConfig::default());
    assert_eq!(v["itens"].as_array().unwrap().len(), 200);
    assert_eq!(v["proxima"], 200);
}

#[test]
fn fs_listar_respeita_raizes_e_negados() {
    let raiz = std::env::temp_dir().join(format!("cli_admin_mcpfs_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&raiz);
    std::fs::create_dir_all(raiz.join("pub").join(".ssh")).unwrap();
    std::fs::write(raiz.join("pub").join("a.txt"), b"abc").unwrap();
    std::fs::write(raiz.join("pub").join("cert.pem"), b"x").unwrap();

    let mut cfg = McpConfig::default();
    // sem raízes configuradas: recusa tudo
    cfg.raizes_permitidas = Vec::new();
    assert!(fs_listar(&raiz.join("pub").display().to_string(), 50, &cfg).is_err());

    cfg.raizes_permitidas = vec![raiz.join("pub").display().to_string()];
    let v = fs_listar(&raiz.join("pub").display().to_string(), 50, &cfg).unwrap();
    let nomes: Vec<&str> = v["itens"].as_array().unwrap().iter().map(|i| i["nome"].as_str().unwrap()).collect();
    assert!(nomes.contains(&"a.txt"));
    assert!(!nomes.contains(&"cert.pem") && !nomes.contains(&".ssh"), "itens negados não aparecem: {nomes:?}");
    assert_eq!(v["omitidos_por_regra"], 2);
    // fora da raiz
    assert!(fs_listar(&raiz.display().to_string(), 50, &cfg).is_err());
    let _ = std::fs::remove_dir_all(&raiz);
}

#[test]
fn fs_listar_nao_devolve_conteudo_de_arquivo() {
    let raiz = std::env::temp_dir().join(format!("cli_admin_mcpfs2_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&raiz);
    std::fs::create_dir_all(&raiz).unwrap();
    std::fs::write(raiz.join("nota.txt"), b"CONTEUDO-SECRETO-123").unwrap();
    let mut cfg = McpConfig::default();
    cfg.raizes_permitidas = vec![raiz.display().to_string()];
    let texto = fs_listar(&raiz.display().to_string(), 50, &cfg).unwrap().to_string();
    assert!(texto.contains("nota.txt") && !texto.contains("CONTEUDO-SECRETO"));
    let _ = std::fs::remove_dir_all(&raiz);
}
