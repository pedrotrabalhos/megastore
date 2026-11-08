//! Catálogo persistido: produtos, clientes e compras.
//!
//! Os dados ficam em vetores (ordem estável, serialização simples) e dois `HashMap`
//! indexam produtos e clientes por id, dando consulta O(1) em vez de busca linear O(n).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Product {
    pub id: u32,
    pub name: String,
    pub category: String,
    pub price: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Customer {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Purchase {
    pub customer_id: u32,
    pub product_id: u32,
    pub quantity: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<u8>,
}

impl Purchase {
    /// Peso da aresta cliente–produto: quantidade × fator da nota (nota ÷ 3; sem nota, 1).
    /// Uma nota 5 vale mais que uma compra neutra; uma nota 1 vale menos.
    pub fn weight(&self) -> f32 {
        let factor = self.rating.map_or(1.0, |rating| f32::from(rating) / 3.0);
        self.quantity as f32 * factor
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    products: Vec<Product>,
    customers: Vec<Customer>,
    purchases: Vec<Purchase>,
    #[serde(skip)]
    product_index: HashMap<u32, usize>,
    #[serde(skip)]
    customer_index: HashMap<u32, usize>,
}

impl Store {
    pub fn from_parts(
        products: Vec<Product>,
        customers: Vec<Customer>,
        purchases: Vec<Purchase>,
    ) -> Result<Self> {
        let mut store = Store { products, customers, purchases, ..Default::default() };
        store.reindex()?;
        Ok(store)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let mut store: Store = serde_json::from_str(&fs::read_to_string(path)?)?;
        store
            .reindex()
            .map_err(|error| Error::Invalid(format!("arquivo de dados inconsistente ({}): {error}", path.display())))?;
        Ok(store)
    }

    /// Grava num arquivo temporário e renomeia: uma interrupção no meio da escrita
    /// nunca deixa o arquivo de dados truncado.
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut temporary = path.as_os_str().to_owned();
        temporary.push(format!(".{}.tmp", std::process::id()));
        fs::write(&temporary, serde_json::to_string_pretty(self)?)?;
        fs::rename(&temporary, path)?;
        Ok(())
    }

    /// Reconstrói os índices e valida que toda compra aponta para ids existentes.
    fn reindex(&mut self) -> Result<()> {
        self.product_index = HashMap::with_capacity(self.products.len());
        for (position, product) in self.products.iter().enumerate() {
            validate_product(&product.name, &product.category, product.price)?;
            if self.product_index.insert(product.id, position).is_some() {
                return Err(Error::Invalid(format!("produto {} duplicado", product.id)));
            }
        }
        self.customer_index = HashMap::with_capacity(self.customers.len());
        for (position, customer) in self.customers.iter().enumerate() {
            validate_name(&customer.name)?;
            if self.customer_index.insert(customer.id, position).is_some() {
                return Err(Error::Invalid(format!("cliente {} duplicado", customer.id)));
            }
        }
        for purchase in &self.purchases {
            self.validate_purchase(purchase)?;
        }
        Ok(())
    }

    fn validate_purchase(&self, purchase: &Purchase) -> Result<()> {
        self.validate_ids(purchase.customer_id, purchase.product_id)?;
        if !(1..=MAX_QUANTITY).contains(&purchase.quantity) {
            return Err(Error::Invalid(format!("a quantidade deve estar entre 1 e {MAX_QUANTITY}")));
        }
        if let Some(rating) = purchase.rating
            && !(1..=5).contains(&rating)
        {
            return Err(Error::Invalid("a nota deve estar entre 1 e 5".into()));
        }
        Ok(())
    }

    fn validate_ids(&self, customer_id: u32, product_id: u32) -> Result<()> {
        if !self.customer_index.contains_key(&customer_id) {
            return Err(Error::CustomerNotFound(customer_id));
        }
        if !self.product_index.contains_key(&product_id) {
            return Err(Error::ProductNotFound(product_id));
        }
        Ok(())
    }

    pub fn products(&self) -> &[Product] {
        &self.products
    }

    pub fn customers(&self) -> &[Customer] {
        &self.customers
    }

    pub fn purchases(&self) -> &[Purchase] {
        &self.purchases
    }

    /// Consulta por id em O(1) médio via `HashMap`.
    pub fn product(&self, id: u32) -> Option<&Product> {
        self.product_index.get(&id).map(|&position| &self.products[position])
    }

    pub fn customer(&self, id: u32) -> Option<&Customer> {
        self.customer_index.get(&id).map(|&position| &self.customers[position])
    }

    pub fn add_product(&mut self, name: &str, category: &str, price: f64) -> Result<u32> {
        let (name, category) = (name.trim(), category.trim());
        validate_product(name, category, price)?;
        // Reaproveita a grafia de uma categoria existente ("eletronicos" → "Eletrônicos"),
        // para não criar dois vértices de categoria para a mesma coisa.
        let normalized = normalize(category);
        let category = self
            .products
            .iter()
            .find(|product| normalize(&product.category) == normalized)
            .map_or_else(|| category.to_string(), |product| product.category.clone());
        let id = self.product_index.keys().max().map_or(1, |max| max + 1);
        self.product_index.insert(id, self.products.len());
        self.products.push(Product { id, name: name.to_string(), category, price });
        Ok(id)
    }

    pub fn add_customer(&mut self, name: &str) -> Result<u32> {
        let name = name.trim();
        validate_name(name)?;
        let id = self.customer_index.keys().max().map_or(1, |max| max + 1);
        self.customer_index.insert(id, self.customers.len());
        self.customers.push(Customer { id, name: name.to_string() });
        Ok(id)
    }

    pub fn add_purchase(&mut self, purchase: Purchase) -> Result<()> {
        self.validate_purchase(&purchase)?;
        self.purchases.push(purchase);
        Ok(())
    }

    /// Remove todas as compras de um cliente para um produto. Devolve quantas foram removidas.
    pub fn remove_purchases(&mut self, customer_id: u32, product_id: u32) -> Result<usize> {
        self.validate_ids(customer_id, product_id)?;
        let before = self.purchases.len();
        self.purchases
            .retain(|purchase| !(purchase.customer_id == customer_id && purchase.product_id == product_id));
        Ok(before - self.purchases.len())
    }

    pub fn purchases_of(&self, customer_id: u32) -> impl Iterator<Item = &Purchase> {
        self.purchases.iter().filter(move |purchase| purchase.customer_id == customer_id)
    }

    /// Quantos clientes distintos compraram o produto.
    pub fn buyer_count(&self, product_id: u32) -> usize {
        self.purchases
            .iter()
            .filter(|purchase| purchase.product_id == product_id)
            .map(|purchase| purchase.customer_id)
            .collect::<HashSet<_>>()
            .len()
    }

    /// Filtra por trecho do nome e/ou categoria, sem diferenciar maiúsculas nem acentos.
    pub fn search(&self, term: Option<&str>, category: Option<&str>) -> Vec<&Product> {
        let term = term.map(normalize);
        let category = category.map(normalize);
        self.products
            .iter()
            .filter(|product| category.as_ref().is_none_or(|c| normalize(&product.category) == *c))
            .filter(|product| term.as_ref().is_none_or(|t| normalize(&product.name).contains(t.as_str())))
            .collect()
    }

    /// Categorias e quantos produtos cada uma tem, em ordem alfabética.
    pub fn categories(&self) -> Vec<(String, usize)> {
        let mut counts = BTreeMap::new();
        for product in &self.products {
            *counts.entry(product.category.clone()).or_insert(0) += 1;
        }
        counts.into_iter().collect()
    }
}

pub const MAX_QUANTITY: u32 = 1_000;
pub const MAX_PRICE: f64 = 1_000_000_000.0;
pub const MAX_NAME: usize = 120;

fn validate_name(name: &str) -> Result<()> {
    let length = name.trim().chars().count();
    if length == 0 || length > MAX_NAME {
        return Err(Error::Invalid(format!("nomes devem ter entre 1 e {MAX_NAME} caracteres")));
    }
    Ok(())
}

fn validate_product(name: &str, category: &str, price: f64) -> Result<()> {
    validate_name(name)?;
    validate_name(category).map_err(|_| Error::Invalid(format!("a categoria deve ter entre 1 e {MAX_NAME} caracteres")))?;
    if !price.is_finite() || !(0.0..=MAX_PRICE).contains(&price) {
        return Err(Error::Invalid("o preço deve estar entre R$ 0 e R$ 1.000.000.000".into()));
    }
    Ok(())
}

/// Minúsculas e sem acentos do português, para comparar textos digitados pelo usuário.
pub fn normalize(text: &str) -> String {
    text.trim()
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        let mut store = Store::default();
        store.add_product("Notebook", "Eletrônicos", 3500.0).unwrap();
        store.add_product("Romance", "Livros", 45.0).unwrap();
        store.add_customer("Ana").unwrap();
        store
    }

    #[test]
    fn ids_sao_sequenciais_e_consultados_por_hashmap() {
        let store = store();
        assert_eq!(store.product(1).unwrap().name, "Notebook");
        assert_eq!(store.product(2).unwrap().name, "Romance");
        assert!(store.product(3).is_none());
        assert_eq!(store.customer(1).unwrap().name, "Ana");
    }

    #[test]
    fn cadastro_valida_campos() {
        let mut store = store();
        assert!(store.add_product("  ", "Livros", 10.0).is_err());
        assert!(store.add_product("HQ", "Livros", -1.0).is_err());
        assert!(store.add_product("HQ", "Livros", f64::NAN).is_err());
        assert!(store.add_customer("").is_err());
    }

    #[test]
    fn categoria_reaproveita_grafia_existente() {
        let mut store = store();
        let id = store.add_product("Fone", "eletronicos", 200.0).unwrap();
        assert_eq!(store.product(id).unwrap().category, "Eletrônicos");
        assert_eq!(store.categories().len(), 2);
    }

    #[test]
    fn busca_ignora_acentos() {
        let mut store = store();
        store.add_product("Café Básico", "Alimentos", 20.0).unwrap();
        assert_eq!(store.search(Some("cafe basico"), None).len(), 1);
        assert_eq!(store.search(None, Some("ELETRONICOS")).len(), 1);
    }

    #[test]
    fn compra_valida_ids_quantidade_e_nota() {
        let mut store = store();
        let purchase = |customer_id, product_id, quantity, rating| Purchase { customer_id, product_id, quantity, rating };
        assert!(matches!(store.add_purchase(purchase(9, 1, 1, None)), Err(Error::CustomerNotFound(9))));
        assert!(matches!(store.add_purchase(purchase(1, 9, 1, None)), Err(Error::ProductNotFound(9))));
        assert!(store.add_purchase(purchase(1, 1, 0, None)).is_err());
        assert!(store.add_purchase(purchase(1, 1, 1, Some(6))).is_err());
        store.add_purchase(purchase(1, 1, 2, Some(5))).unwrap();
        store.add_purchase(purchase(1, 1, 1, None)).unwrap();
        assert_eq!(store.purchases_of(1).count(), 2);
        assert_eq!(store.buyer_count(1), 1);
        assert_eq!(store.remove_purchases(1, 1).unwrap(), 2);
        assert_eq!(store.purchases().len(), 0);
    }

    #[test]
    fn peso_considera_quantidade_e_nota() {
        let purchase = |quantity, rating| Purchase { customer_id: 1, product_id: 1, quantity, rating };
        assert_eq!(purchase(1, None).weight(), 1.0);
        assert_eq!(purchase(2, Some(3)).weight(), 2.0);
        assert!(purchase(1, Some(5)).weight() > purchase(1, Some(1)).weight());
    }

    #[test]
    fn busca_por_nome_e_categoria() {
        let store = store();
        assert_eq!(store.search(Some("note"), None).len(), 1);
        assert_eq!(store.search(None, Some("livros")).len(), 1);
        assert_eq!(store.search(Some("note"), Some("Livros")).len(), 0);
    }

    #[test]
    fn salva_e_carrega_json() {
        let mut store = store();
        store.add_purchase(Purchase { customer_id: 1, product_id: 2, quantity: 1, rating: Some(4) }).unwrap();
        let path = std::env::temp_dir().join(format!("megastore-store-{}.json", std::process::id()));
        store.save(&path).unwrap();
        let loaded = Store::load(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(loaded.products(), store.products());
        assert_eq!(loaded.purchases(), store.purchases());
        assert_eq!(loaded.product(2).unwrap().name, "Romance");
    }

    #[test]
    fn rejeita_compra_com_id_inexistente_ao_carregar() {
        let purchases = vec![Purchase { customer_id: 1, product_id: 99, quantity: 1, rating: None }];
        let customers = vec![Customer { id: 1, name: "Ana".into() }];
        assert!(Store::from_parts(vec![], customers, purchases).is_err());
    }
}
