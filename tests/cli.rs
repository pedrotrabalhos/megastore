//! Executa o binário como o usuário faria, com um arquivo de dados temporário.

use std::path::PathBuf;
use std::process::{Command, Output};

struct Workspace(PathBuf);

impl Workspace {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("megastore-cli-{}-{name}.json", std::process::id()));
        std::fs::remove_file(&path).ok();
        Workspace(path)
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_megastore"))
            .arg("--data")
            .arg(&self.0)
            .args(args)
            .output()
            .expect("executa o binário")
    }

    fn ok(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(output.status.success(), "{args:?} falhou: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).unwrap()
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).ok();
    }
}

#[test]
fn cria_base_de_exemplo_quando_o_arquivo_nao_existe() {
    let workspace = Workspace::new("auto");
    let output = workspace.ok(&["product", "list"]);
    assert!(output.contains("de 48 produto(s)"));
    assert!(workspace.0.exists());
}

#[test]
fn cadastro_e_consulta_persistem_entre_execucoes() {
    let workspace = Workspace::new("crud");
    workspace.ok(&["seed", "--products", "16", "--customers", "4"]);
    let added = workspace.ok(&["product", "add", "--name", "Mouse Gamer", "--category", "eletrônicos", "--price", "149.9"]);
    assert!(added.contains("Produto 17 cadastrado: Mouse Gamer (Eletrônicos, R$ 149,90)"));

    let found = workspace.ok(&["product", "list", "--search", "mouse"]);
    assert!(found.contains("Mouse Gamer") && found.contains("1 de 1 produto(s)"));
    assert!(workspace.ok(&["product", "show", "17"]).contains("Compradores: 0"));
}

#[test]
fn compra_cria_aresta_e_gera_recomendacao() {
    let workspace = Workspace::new("purchase");
    workspace.ok(&["seed"]);
    workspace.ok(&["customer", "add", "--name", "Pedro"]);
    assert!(workspace.ok(&["recommend", "customer", "21"]).contains("Nenhuma recomendação"));

    let bought = workspace.ok(&["purchase", "add", "21", "2", "--rating", "5"]);
    assert!(bought.contains("Compra registrada: Pedro → Camiseta Básico"));
    let recommended = workspace.ok(&["recommend", "customer", "21"]);
    assert!(recommended.contains("porque você comprou Camiseta Básico"));
    assert!(!recommended.contains("Nenhuma recomendação"));

    assert!(workspace.ok(&["purchase", "remove", "21", "2"]).contains("1 compra(s) removida(s)"));
    assert!(workspace.ok(&["recommend", "customer", "21"]).contains("Nenhuma recomendação"));
}

#[test]
fn erros_saem_com_codigo_diferente_de_zero() {
    let workspace = Workspace::new("errors");
    workspace.ok(&["seed"]);
    for args in [
        &["product", "show", "999"][..],
        &["recommend", "customer", "999"],
        &["purchase", "add", "1", "999"],
        &["purchase", "add", "1", "1", "--rating", "9"],
        &["product", "add", "--name", "X", "--category", "Y", "--price=-5"],
    ] {
        let output = workspace.run(args);
        assert!(!output.status.success(), "{args:?} deveria falhar");
        assert!(String::from_utf8_lossy(&output.stderr).starts_with("erro:"));
    }
}

#[test]
fn entrada_em_portugues_e_tolerante_a_acentos_e_virgula() {
    let workspace = Workspace::new("ptbr");
    workspace.ok(&["seed"]);
    assert!(workspace.ok(&["product", "list", "--category", "eletronicos"]).contains("6 de 6 produto(s)"));
    assert!(workspace.ok(&["product", "list", "--search", "cafe"]).contains("Café em Grãos"));
    let added = workspace.ok(&["product", "add", "--name", "Mouse", "--category", "ELETRONICOS", "--price", "10,5"]);
    assert!(added.contains("(Eletrônicos, R$ 10,50)"));
    assert_eq!(workspace.ok(&["product", "categories"]).matches("Eletrônicos").count(), 1);
    assert!(workspace.ok(&["product", "list", "--search", "inexistente"]).contains("Nenhum produto encontrado"));
}

#[test]
fn rejeita_k_zero_e_profundidade_insuficiente() {
    let workspace = Workspace::new("args");
    workspace.ok(&["seed"]);
    for args in [
        &["recommend", "customer", "1", "-k", "0"][..],
        &["recommend", "customer", "1", "--depth", "2"],
        &["recommend", "product", "1", "--depth", "1"],
        &["recommend", "customer", "1", "--depth", "10"],
    ] {
        assert!(!workspace.run(args).status.success(), "{args:?} deveria falhar");
    }
}

#[test]
fn bench_mede_volumes_pequenos() {
    let workspace = Workspace::new("bench");
    let output = workspace.ok(&["bench", "--sizes", "100,1000", "--samples", "10"]);
    assert!(output.contains("Volume e memória") && output.contains("Busca HashMap"));
}
