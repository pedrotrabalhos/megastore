//! Interface de linha de comando: cada subcomando executa e termina.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};

use crate::bench;
use crate::graph::{Graph, Node};
use crate::recommend::{self, Outcome, Reason};
use crate::seed::{self, SeedConfig};
use crate::store::{Purchase, Store};
use crate::{Error, Result};

#[derive(Parser)]
#[command(name = "megastore", version, about = "ConectaStore — recomendação de produtos baseada em grafos")]
struct Cli {
    /// Arquivo JSON com produtos, clientes e compras (criado com dados de exemplo se não existir)
    #[arg(long, global = true, default_value = "megastore.json")]
    data: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Gera uma base sintética e sobrescreve o arquivo de dados
    Seed {
        #[arg(long, default_value_t = 48, value_parser = count::<0, 2_000_000>)]
        products: usize,
        #[arg(long, default_value_t = 20, value_parser = count::<0, 1_000_000>)]
        customers: usize,
        /// Média de compras por cliente
        #[arg(long, default_value_t = 5, value_parser = count::<0, 100>)]
        purchases: usize,
        #[arg(long, default_value_t = 42)]
        seed: u64,
    },
    /// Cadastro e consulta de produtos
    #[command(subcommand)]
    Product(ProductCommand),
    /// Cadastro e consulta de clientes
    #[command(subcommand)]
    Customer(CustomerCommand),
    /// Compras: as arestas cliente–produto do grafo
    #[command(subcommand)]
    Purchase(PurchaseCommand),
    /// Recomendações a partir de um cliente ou de um produto
    #[command(subcommand)]
    Recommend(RecommendCommand),
    /// Estatísticas do grafo e memória estimada (lista × matriz de adjacência)
    Stats,
    /// Mede o tempo de execução com volumes diferentes (rode com --release)
    Bench {
        /// Quantidades de produtos, separadas por vírgula
        #[arg(long, value_delimiter = ',', default_values_t = [1_000, 10_000, 100_000, 1_000_000], value_parser = count::<1, 5_000_000>)]
        sizes: Vec<usize>,
        /// Consultas e recomendações aleatórias medidas por volume
        #[arg(long, default_value_t = 200, value_parser = count::<1, 100_000>)]
        samples: usize,
    },
}

#[derive(Subcommand)]
enum ProductCommand {
    /// Lista produtos, com filtros opcionais
    List {
        #[arg(long)]
        category: Option<String>,
        /// Trecho do nome
        #[arg(long)]
        search: Option<String>,
        #[arg(long, default_value_t = 50, value_parser = count::<1, 1_000_000>)]
        limit: usize,
    },
    /// Mostra um produto e o que foi comprado junto com ele
    Show {
        id: u32,
        #[arg(short, default_value_t = 5, value_parser = count::<1, 100>)]
        k: usize,
    },
    /// Cadastra um produto
    Add {
        #[arg(long)]
        name: String,
        #[arg(long)]
        category: String,
        /// Preço (aceita vírgula decimal: 149,90)
        #[arg(long, allow_hyphen_values = true, value_parser = price)]
        price: f64,
    },
    /// Lista as categorias
    Categories,
}

#[derive(Subcommand)]
enum CustomerCommand {
    /// Lista clientes
    List,
    /// Mostra o histórico de um cliente e as recomendações para ele
    Show {
        id: u32,
        #[arg(short, default_value_t = 5, value_parser = count::<1, 100>)]
        k: usize,
    },
    /// Cadastra um cliente
    Add {
        #[arg(long)]
        name: String,
    },
}

#[derive(Subcommand)]
enum PurchaseCommand {
    /// Registra uma compra (cria ou reforça a aresta cliente–produto)
    Add {
        customer: u32,
        product: u32,
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=1000))]
        quantity: u32,
        /// Nota de 1 a 5
        #[arg(long)]
        rating: Option<u8>,
    },
    /// Remove as compras de um cliente para um produto (remove a aresta)
    Remove { customer: u32, product: u32 },
}

#[derive(Subcommand)]
enum RecommendCommand {
    /// "Recomendado para você": a partir do histórico do cliente
    Customer {
        id: u32,
        #[arg(short, default_value_t = 5, value_parser = count::<1, 100>)]
        k: usize,
        /// Níveis da BFS, de 3 a 9 (3 = cliente → produtos → clientes → produtos)
        #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u8).range(3..=9))]
        depth: u8,
    },
    /// "Quem comprou também comprou": a partir de um produto
    Product {
        id: u32,
        #[arg(short, default_value_t = 5, value_parser = count::<1, 100>)]
        k: usize,
        /// Níveis da BFS, de 2 a 8 (2 = produto → clientes → produtos)
        #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u8).range(2..=8))]
        depth: u8,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let path = cli.data.as_path();
    match cli.command {
        Command::Seed { products, customers, purchases, seed } => {
            let store = seed::generate(&SeedConfig { products, customers, purchases_per_customer: purchases, seed });
            store.save(path)?;
            println!(
                "Base gerada em {}: {} produtos, {} clientes, {} compras.",
                path.display(),
                store.products().len(),
                store.customers().len(),
                store.purchases().len()
            );
            Ok(())
        }
        Command::Product(command) => product(command, path),
        Command::Customer(command) => customer(command, path),
        Command::Purchase(command) => purchase(command, path),
        Command::Recommend(command) => recommendation(command, path),
        Command::Stats => stats(&open(path)?),
        Command::Bench { sizes, samples } => {
            benchmark(&sizes, samples);
            Ok(())
        }
    }
}

fn open(path: &Path) -> Result<Store> {
    if path.exists() {
        return Store::load(path);
    }
    let store = seed::generate(&SeedConfig::default());
    store.save(path)?;
    eprintln!("{} não existia: base de exemplo gerada (use `megastore seed` para gerar outra).\n", path.display());
    Ok(store)
}

fn product(command: ProductCommand, path: &Path) -> Result<()> {
    let mut store = open(path)?;
    match command {
        ProductCommand::List { category, search, limit } => {
            let found = store.search(search.as_deref(), category.as_deref());
            let rows: Vec<Vec<String>> = found
                .iter()
                .take(limit)
                .map(|p| vec![p.id.to_string(), p.name.clone(), p.category.clone(), money(p.price)])
                .collect();
            if rows.is_empty() {
                println!("Nenhum produto encontrado (veja as categorias com `product categories`).");
            } else {
                print_table(&["ID", "Nome", "Categoria", "Preço"], &rows);
                println!("\n{} de {} produto(s).", rows.len(), found.len());
            }
        }
        ProductCommand::Show { id, k } => {
            let product = store.product(id).ok_or(Error::ProductNotFound(id))?;
            let graph = Graph::from_store(&store);
            let started = Instant::now();
            let outcome = recommend::for_product(&graph, id, k, 2).ok_or(Error::ProductNotFound(id))?;
            let elapsed = started.elapsed();
            println!("Produto {} — {}", product.id, product.name);
            println!("Categoria:   {}", product.category);
            println!("Preço:       {}", money(product.price));
            println!("Compradores: {}", store.buyer_count(id));
            println!("\nQuem comprou também comprou:\n");
            print_recommendations(&store, &outcome, elapsed);
        }
        ProductCommand::Add { name, category, price } => {
            let id = store.add_product(&name, &category, price)?;
            store.save(path)?;
            let product = store.product(id).expect("produto recém-cadastrado");
            println!("Produto {id} cadastrado: {} ({}, {}).", product.name, product.category, money(product.price));
        }
        ProductCommand::Categories => {
            let rows: Vec<Vec<String>> =
                store.categories().into_iter().map(|(name, count)| vec![name, count.to_string()]).collect();
            print_table(&["Categoria", "Produtos"], &rows);
        }
    }
    Ok(())
}

fn customer(command: CustomerCommand, path: &Path) -> Result<()> {
    let mut store = open(path)?;
    match command {
        CustomerCommand::List => {
            let mut counts: HashMap<u32, usize> = HashMap::new();
            for purchase in store.purchases() {
                *counts.entry(purchase.customer_id).or_default() += 1;
            }
            let rows: Vec<Vec<String>> = store
                .customers()
                .iter()
                .map(|c| vec![c.id.to_string(), c.name.clone(), counts.get(&c.id).copied().unwrap_or(0).to_string()])
                .collect();
            print_table(&["ID", "Nome", "Compras"], &rows);
        }
        CustomerCommand::Show { id, k } => {
            let customer = store.customer(id).ok_or(Error::CustomerNotFound(id))?;
            println!("Cliente {} — {}\n\nHistórico de compras:\n", customer.id, customer.name);
            // Compras repetidas do mesmo produto viram uma linha (como a aresta única do grafo):
            // quantidade somada e média das notas dadas.
            let mut history: Vec<(u32, u32, Vec<u8>)> = Vec::new();
            for purchase in store.purchases_of(id) {
                match history.iter_mut().find(|(product, _, _)| *product == purchase.product_id) {
                    Some((_, quantity, ratings)) => {
                        *quantity += purchase.quantity;
                        ratings.extend(purchase.rating);
                    }
                    None => history.push((purchase.product_id, purchase.quantity, purchase.rating.into_iter().collect())),
                }
            }
            let rows: Vec<Vec<String>> = history
                .iter()
                .map(|(product_id, quantity, ratings)| {
                    let product = store.product(*product_id).expect("compra validada");
                    let rating = if ratings.is_empty() {
                        "—".to_string()
                    } else {
                        let average = ratings.iter().map(|&r| f64::from(r)).sum::<f64>() / ratings.len() as f64;
                        decimal(average, if ratings.len() == 1 { 0 } else { 1 })
                    };
                    vec![product.id.to_string(), product.name.clone(), product.category.clone(), quantity.to_string(), rating]
                })
                .collect();
            if rows.is_empty() {
                println!("(nenhuma compra)");
            } else {
                print_table(&["ID", "Produto", "Categoria", "Qtd", "Nota"], &rows);
            }
            let graph = Graph::from_store(&store);
            let started = Instant::now();
            let outcome = recommend::for_customer(&graph, id, k, 3).ok_or(Error::CustomerNotFound(id))?;
            let elapsed = started.elapsed();
            println!("\nRecomendado para você:\n");
            print_recommendations(&store, &outcome, elapsed);
        }
        CustomerCommand::Add { name } => {
            let id = store.add_customer(&name)?;
            store.save(path)?;
            println!("Cliente {id} cadastrado: {}.", name.trim());
        }
    }
    Ok(())
}

fn purchase(command: PurchaseCommand, path: &Path) -> Result<()> {
    let mut store = open(path)?;
    match command {
        PurchaseCommand::Add { customer, product, quantity, rating } => {
            let purchase = Purchase { customer_id: customer, product_id: product, quantity, rating };
            let weight = purchase.weight();
            store.add_purchase(purchase)?;
            store.save(path)?;
            println!(
                "Compra registrada: {} → {} (aresta cliente–produto, +{} de peso).",
                store.customer(customer).expect("cliente validado").name,
                store.product(product).expect("produto validado").name,
                decimal(f64::from(weight), 2),
            );
        }
        PurchaseCommand::Remove { customer, product } => {
            let removed = store.remove_purchases(customer, product)?;
            if removed == 0 {
                return Err(Error::Invalid(format!("o cliente {customer} não comprou o produto {product}")));
            }
            store.save(path)?;
            println!("{removed} compra(s) removida(s): a aresta cliente {customer} – produto {product} saiu do grafo.");
        }
    }
    Ok(())
}

fn recommendation(command: RecommendCommand, path: &Path) -> Result<()> {
    let store = open(path)?;
    let graph = Graph::from_store(&store);
    let started = Instant::now();
    let (title, outcome) = match command {
        RecommendCommand::Customer { id, k, depth } => {
            let customer = store.customer(id).ok_or(Error::CustomerNotFound(id))?;
            let outcome = recommend::for_customer(&graph, id, k, depth).ok_or(Error::CustomerNotFound(id))?;
            (format!("Recomendações para {} (cliente {id}), BFS com profundidade {depth}", customer.name), outcome)
        }
        RecommendCommand::Product { id, k, depth } => {
            let product = store.product(id).ok_or(Error::ProductNotFound(id))?;
            let outcome = recommend::for_product(&graph, id, k, depth).ok_or(Error::ProductNotFound(id))?;
            (format!("Quem comprou {} (produto {id}) também comprou, BFS com profundidade {depth}", product.name), outcome)
        }
    };
    let elapsed = started.elapsed();
    println!("{title}\n");
    print_recommendations(&store, &outcome, elapsed);
    Ok(())
}

fn stats(store: &Store) -> Result<()> {
    let started = Instant::now();
    let graph = Graph::from_store(store);
    let build = started.elapsed();

    let (mut customers, mut products, mut categories) = (0, 0, 0);
    let mut busiest: Option<(usize, usize)> = None;
    for (index, node) in graph.nodes() {
        match node {
            Node::Customer(_) => customers += 1,
            Node::Category(_) => categories += 1,
            Node::Product(_) => {
                products += 1;
                let buyers = graph.neighbors(index).len() - 1;
                if busiest.is_none_or(|(_, most)| buyers > most) {
                    busiest = Some((index, buyers));
                }
            }
        }
    }
    let memory = graph.memory_estimate();
    let nodes = graph.node_count();
    let edges = graph.edge_count();

    println!("Vértices: {nodes} ({customers} clientes, {products} produtos, {categories} categorias)");
    println!("Arestas:  {edges} ({} cliente–produto, {products} produto–categoria)", edges - products);
    println!("Grau médio: {}", decimal(if nodes == 0 { 0.0 } else { 2.0 * edges as f64 / nodes as f64 }, 2));
    if let Some((index, buyers)) = busiest
        && let Node::Product(id) = graph.node(index)
    {
        let name = store.product(*id).map_or("?", |p| p.name.as_str());
        println!("Produto mais conectado: {name} ({buyers} compradores)");
    }
    println!("Memória estimada: lista de adjacência {}, matriz de adjacência {}", bytes(memory.adjacency_list), bytes(memory.adjacency_matrix));
    println!("Construção do grafo: {}", duration(build));
    Ok(())
}

fn benchmark(sizes: &[usize], samples: usize) {
    if cfg!(debug_assertions) {
        eprintln!("aviso: compilado sem --release; os tempos medidos serão bem maiores.\n");
    }
    let rows: Vec<bench::BenchRow> = sizes
        .iter()
        .map(|&size| {
            eprintln!("Medindo {size} produtos...");
            bench::run(size, samples, 42)
        })
        .collect();

    println!("\nVolume e memória\n");
    let table: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            vec![
                r.products.to_string(),
                r.customers.to_string(),
                r.purchases.to_string(),
                r.nodes.to_string(),
                r.edges.to_string(),
                bytes(r.list_bytes),
                bytes(r.matrix_bytes),
            ]
        })
        .collect();
    print_table(&["Produtos", "Clientes", "Compras", "Vértices", "Arestas", "Lista adj.", "Matriz adj."], &table);

    println!("\nTempo (médias de {samples} execuções)\n");
    let table: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            vec![
                r.products.to_string(),
                duration(r.build),
                duration(r.hash_lookup),
                duration(r.linear_lookup),
                duration(r.recommend_customer),
                duration(r.recommend_product),
                format!("{:.0}", r.visited_average),
            ]
        })
        .collect();
    print_table(
        &["Produtos", "Construção", "Busca HashMap", "Busca linear", "Rec. cliente", "Rec. produto", "Visitados (cliente)"],
        &table,
    );
}

fn print_recommendations(store: &Store, outcome: &Outcome, elapsed: Duration) {
    if outcome.items.is_empty() {
        println!("Nenhuma recomendação: não há compras relacionadas nem outros produtos na categoria.");
    } else {
        let rows: Vec<Vec<String>> = outcome
            .items
            .iter()
            .enumerate()
            .map(|(position, item)| {
                let product = store.product(item.product_id);
                let score = match item.reason {
                    Reason::SameCategory { .. } => "—".to_string(),
                    _ => decimal(f64::from(item.score), 4),
                };
                vec![
                    (position + 1).to_string(),
                    item.product_id.to_string(),
                    product.map_or("?".into(), |p| p.name.clone()),
                    product.map_or("?".into(), |p| p.category.clone()),
                    score,
                    reason(store, &item.reason),
                ]
            })
            .collect();
        print_table(&["#", "ID", "Produto", "Categoria", "Score", "Motivo"], &rows);
    }
    println!("\nBFS visitou {} vértices em {}.", outcome.visited, duration(elapsed));
}

fn reason(store: &Store, reason: &Reason) -> String {
    match reason {
        Reason::BoughtWith { via } => {
            format!("porque você comprou {}", store.product(*via).map_or("?", |p| p.name.as_str()))
        }
        Reason::AlsoBought { shared: 1 } => "1 cliente comprou os dois".into(),
        Reason::AlsoBought { shared } => format!("{shared} clientes compraram os dois"),
        Reason::Indirect { distance } => format!("conexão indireta (distância {distance})"),
        Reason::SameCategory { category } => format!("popular na mesma categoria ({category})"),
    }
}

fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    let mut widths: Vec<usize> = headers.iter().map(|header| header.chars().count()).collect();
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    let render = |cells: &mut dyn Iterator<Item = &str>| {
        let line: Vec<String> = cells.zip(&widths).map(|(cell, &width)| format!("{cell:<width$}")).collect();
        println!("{}", line.join("  ").trim_end());
    };
    render(&mut headers.iter().copied());
    let separator: Vec<String> = widths.iter().map(|&width| "─".repeat(width)).collect();
    render(&mut separator.iter().map(String::as_str));
    for row in rows {
        render(&mut row.iter().map(String::as_str));
    }
}

/// Inteiro dentro de `MIN..=MAX`: limita a entrada antes que vire alocação gigante.
fn count<const MIN: usize, const MAX: usize>(text: &str) -> std::result::Result<usize, String> {
    match text.parse::<usize>() {
        Ok(value) if (MIN..=MAX).contains(&value) => Ok(value),
        _ => Err(format!("deve ser um inteiro entre {MIN} e {MAX}")),
    }
}

/// Aceita "149.90" e "149,90".
fn price(text: &str) -> std::result::Result<f64, String> {
    text.replace(',', ".").parse().map_err(|_| "preço inválido (ex.: 149,90)".to_string())
}

/// Número com vírgula decimal, como no português.
fn decimal(value: f64, places: usize) -> String {
    format!("{value:.places$}").replace('.', ",")
}

fn money(value: f64) -> String {
    format!("R$ {}", decimal(value, 2))
}

fn duration(value: Duration) -> String {
    let nanos = value.as_nanos();
    match nanos {
        0..1_000 => format!("{nanos} ns"),
        1_000..1_000_000 => format!("{} µs", decimal(nanos as f64 / 1e3, 1)),
        1_000_000..1_000_000_000 => format!("{} ms", decimal(nanos as f64 / 1e6, 1)),
        _ => format!("{} s", decimal(nanos as f64 / 1e9, 2)),
    }
}

fn bytes(value: u128) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut size = value as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 { format!("{value} B") } else { format!("{} {}", decimal(size, 1), UNITS[unit]) }
}
