use anyhow::{Context, Result};
use parking_lot::Mutex;
use std::path::Path;
use tantivy::collector::TopDocs;
use tantivy::directory::MmapDirectory;
use tantivy::query::QueryParser;
use tantivy::{schema::*, Index, IndexReader, IndexWriter, TantivyDocument, Term};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SearchError {
    #[error("Tantivy internal error: {0}")]
    Tantivy(#[from] tantivy::TantivyError),
    #[error("Query parsing error: {0}")]
    Query(#[from] tantivy::query::QueryParserError),
    #[error("Unknown indexing error: {0}")]
    Unknown(#[from] anyhow::Error),
}

pub struct BrainSearchEngine {
    index: Index,
    writer: Mutex<IndexWriter>,
    reader: IndexReader,
    schema: Schema,
}

impl BrainSearchEngine {
    pub fn new(storage_path: &Path) -> Result<Self> {
        let mut schema_builder = Schema::builder();

        // The unique Memory Entry ID
        schema_builder.add_text_field("id", STRING | STORED);
        // The project boundary this memory belongs to
        schema_builder.add_text_field("project_id", STRING);
        // Full text body mapped for intense BM25 NLP search
        schema_builder.add_text_field("body", TEXT | STORED);
        // Semantic Tags
        schema_builder.add_text_field("tags", TEXT);

        let schema = schema_builder.build();

        // Use memory mapping for high-perf persistence native to Tantivy
        let directory = MmapDirectory::open(storage_path)
            .context("Failed to open Mmap Directory for Tantivy")?;

        let index = Index::open_or_create(directory, schema.clone())?;

        // 50MB heap allocation for blazing fast batch writes
        let writer = index.writer(50_000_000)?;
        let reader = index.reader()?;

        Ok(Self {
            index,
            writer: Mutex::new(writer),
            reader,
            schema,
        })
    }

    pub fn insert_memory(
        &self,
        id: &str,
        project_id: &str,
        body: &str,
        tags: &str,
    ) -> Result<()> {
        let id_field = self.schema.get_field("id").unwrap();
        let pid_field = self.schema.get_field("project_id").unwrap();
        let body_field = self.schema.get_field("body").unwrap();
        let tags_field = self.schema.get_field("tags").unwrap();

        let mut doc = TantivyDocument::default();
        doc.add_text(id_field, id);
        doc.add_text(pid_field, project_id);
        doc.add_text(body_field, body);
        doc.add_text(tags_field, tags);

        let writer = self.writer.lock();
        writer.add_document(doc)?;
        Ok(())
    }

    pub fn delete_memory(&self, id: &str) -> Result<()> {
        let id_field = self.schema.get_field("id").unwrap();
        let term = Term::from_field_text(id_field, id);
        let writer = self.writer.lock();
        writer.delete_term(term);
        Ok(())
    }

    pub fn commit(&self) -> Result<()> {
        let mut writer = self.writer.lock();
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    pub fn search(&self, query_str: &str, limit: usize) -> Result<Vec<String>> {
        let searcher = self.reader.searcher();

        let body_field = self.schema.get_field("body").unwrap();
        let id_field = self.schema.get_field("id").unwrap();

        let query_parser = QueryParser::for_index(&self.index, vec![body_field]);
        let query = query_parser.parse_query(query_str)?;

        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

        let mut results = Vec::new();
        for (_score, doc_address) in top_docs {
            let retrieved_doc: TantivyDocument = searcher.doc(doc_address)?;
            if let Some(id_val) = retrieved_doc.get_first(id_field) {
                if let Some(id_str) = id_val.as_str() {
                    results.push(id_str.to_string());
                }
            }
        }

        Ok(results)
    }
}
