//! Gerador determinístico de dados sintéticos.
//!
//! Cada cliente tem duas categorias favoritas (80% das compras) e, dentro da categoria,
//! a escolha segue uma distribuição enviesada (u², com u uniforme): poucos produtos
//! concentram muitas compras, como num catálogo real. Isso cria comunidades no grafo,
//! que são justamente o que a recomendação explora.

use crate::store::{Customer, Product, Purchase, Store};

const CATEGORIES: [(&str, f64, [&str; 6]); 8] = [
    ("Eletrônicos", 900.0, ["Fone Bluetooth", "Smartphone", "Notebook", "Smartwatch", "Caixa de Som", "Teclado Mecânico"]),
    ("Vestuário", 120.0, ["Camiseta", "Calça Jeans", "Jaqueta", "Tênis", "Moletom", "Vestido"]),
    ("Alimentos", 35.0, ["Café em Grãos", "Chocolate Amargo", "Azeite", "Granola", "Chá Verde", "Mel"]),
    ("Livros", 60.0, ["Romance", "Livro de Receitas", "HQ", "Biografia", "Livro de Programação", "Ficção Científica"]),
    ("Móveis", 700.0, ["Cadeira", "Mesa", "Estante", "Sofá", "Escrivaninha", "Poltrona"]),
    ("Decoração", 90.0, ["Luminária", "Vaso", "Quadro", "Tapete", "Espelho", "Almofada"]),
    ("Esportes", 150.0, ["Bola", "Halteres", "Tapete de Yoga", "Bicicleta", "Garrafa Térmica", "Corda de Pular"]),
    ("Beleza", 70.0, ["Perfume", "Hidratante", "Shampoo", "Protetor Solar", "Batom", "Sabonete"]),
];
const VARIANTS: [&str; 6] = ["Básico", "Plus", "Pro", "Compacto", "Clássico", "Premium"];
const FIRST_NAMES: [&str; 20] = [
    "Ana", "Bruno", "Carla", "Diego", "Eduarda", "Felipe", "Gabriela", "Heitor", "Isabela", "João", "Larissa",
    "Marcos", "Natália", "Otávio", "Paula", "Rafael", "Sofia", "Thiago", "Vitória", "Wesley",
];
const LAST_NAMES: [&str; 10] = ["Silva", "Souza", "Oliveira", "Santos", "Lima", "Costa", "Pereira", "Almeida", "Rocha", "Gomes"];
const RATINGS: [u8; 8] = [1, 2, 3, 4, 4, 5, 5, 5];

/// xorshift64*: pequeno, rápido e reprodutível (mesma semente, mesmos dados).
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Inteiro uniforme em `0..n` (n > 0).
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    /// Real uniforme em `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SeedConfig {
    pub products: usize,
    pub customers: usize,
    /// Média de compras por cliente.
    pub purchases_per_customer: usize,
    pub seed: u64,
}

impl Default for SeedConfig {
    fn default() -> Self {
        Self { products: 48, customers: 20, purchases_per_customer: 5, seed: 42 }
    }
}

pub fn generate(config: &SeedConfig) -> Store {
    let mut rng = Rng::new(config.seed);
    let category_count = CATEGORIES.len();

    // O produto i fica na categoria i % 8, então os produtos da categoria c são c, c+8, c+16...
    let products = (0..config.products)
        .map(|i| {
            let (category, base_price, nouns) = CATEGORIES[i % category_count];
            let j = i / category_count;
            let mut name = format!("{} {}", nouns[j % nouns.len()], VARIANTS[(j / nouns.len()) % VARIANTS.len()]);
            let batch = j / (nouns.len() * VARIANTS.len());
            if batch > 0 {
                name.push_str(&format!(" {}", batch + 1));
            }
            let price = (base_price * (0.5 + rng.unit()) * 100.0).round() / 100.0;
            Product { id: i as u32 + 1, name, category: category.to_string(), price }
        })
        .collect();

    let customers = (0..config.customers)
        .map(|i| {
            let mut name = format!("{} {}", FIRST_NAMES[i % FIRST_NAMES.len()], LAST_NAMES[(i / FIRST_NAMES.len()) % LAST_NAMES.len()]);
            let batch = i / (FIRST_NAMES.len() * LAST_NAMES.len());
            if batch > 0 {
                name.push_str(&format!(" {}", batch + 1));
            }
            Customer { id: i as u32 + 1, name }
        })
        .collect();

    let mut purchases = Vec::with_capacity(config.customers * config.purchases_per_customer);
    if config.products > 0 {
        for customer in 0..config.customers {
            let favorites = [rng.below(category_count), rng.below(category_count)];
            // Todo cliente compra ao menos uma vez (média ≈ `purchases_per_customer`); com média 0, nenhuma.
            let count = match config.purchases_per_customer {
                0 => 0,
                average => 1 + rng.below(2 * average),
            };
            for _ in 0..count {
                let category = if rng.unit() < 0.8 { favorites[rng.below(2)] } else { rng.below(category_count) };
                let in_category = (config.products + category_count - 1 - category) / category_count;
                if in_category == 0 {
                    continue;
                }
                let u = rng.unit();
                let j = ((u * u * in_category as f64) as usize).min(in_category - 1);
                let quantity = if rng.unit() < 0.8 { 1 } else { 2 + rng.below(2) as u32 };
                let rating = (rng.unit() < 0.5).then(|| RATINGS[rng.below(RATINGS.len())]);
                purchases.push(Purchase {
                    customer_id: customer as u32 + 1,
                    product_id: (category + j * category_count) as u32 + 1,
                    quantity,
                    rating,
                });
            }
        }
    }

    Store::from_parts(products, customers, purchases).expect("dados gerados são consistentes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesma_semente_gera_os_mesmos_dados() {
        let config = SeedConfig { products: 200, customers: 50, purchases_per_customer: 4, seed: 7 };
        let (a, b) = (generate(&config), generate(&config));
        assert_eq!(a.products(), b.products());
        assert_eq!(a.purchases(), b.purchases());
        assert_eq!(a.products().len(), 200);
        assert_eq!(a.customers().len(), 50);
        assert!(!a.purchases().is_empty());
    }

    #[test]
    fn base_padrao_tem_nomes_unicos() {
        let store = generate(&SeedConfig::default());
        let names: std::collections::HashSet<_> = store.products().iter().map(|p| &p.name).collect();
        assert_eq!(names.len(), store.products().len());
    }

    #[test]
    fn media_zero_nao_gera_compras() {
        let store = generate(&SeedConfig { purchases_per_customer: 0, ..SeedConfig::default() });
        assert!(store.purchases().is_empty());
    }

    #[test]
    fn catalogo_vazio_nao_gera_compras() {
        let store = generate(&SeedConfig { products: 0, customers: 5, purchases_per_customer: 3, seed: 1 });
        assert!(store.purchases().is_empty());
    }
}
