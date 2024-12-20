use crate::document::DocType;
use crate::edgar::{self, filing};
use crate::memory::{Conversation, ConversationManager, DatabaseMemory, MessageRole};
use crate::query::Query;
use crate::{earnings, ProgressTracker, TokenUsage};
use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use futures::{FutureExt, StreamExt};
use indicatif::MultiProgress;
use itertools::Itertools;
use langchain_rust::schemas::Document;
use langchain_rust::vectorstore::pgvector::{PgFilter::*, PgLit::*, Store};
use langchain_rust::vectorstore::VectorStore;
use langchain_rust::{
    chain::builder::ConversationalChainBuilder,
    chain::Chain,
    llm::{OpenAI, OpenAIConfig},
    prompt_args,
};
use sqlx::{Pool, Postgres};
use std::collections::{HashMap, HashSet};
use std::error::Error as _;
use std::fs;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::str::FromStr as _;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Process documents based on the query parameters
///
/// Logical steps:
/// 1. Create a new MultiProgress tracker for overall progress
/// 2. Initialize a vector to store processing futures
/// 3. If filings are requested in query parameters:
///    - Convert query to EDGAR format
///    - For each ticker:
///      - Fetch matching filings
///      - Process EDGAR filings with progress tracking
/// 4. If earnings data is requested:
///    - Convert query to earnings format
///    - Fetch transcripts for date range
///    - Process earnings transcripts
/// 5. Wait for all futures to complete
/// 6. Clear progress bars if present
async fn process_documents(
    query: &Query,
    http_client: &reqwest::Client,
    store: Arc<Store>,
    progress: Option<&Arc<MultiProgress>>,
) -> Result<()> {
    let progress_tracker = Arc::new(ProgressTracker::new(
        progress,
        &format!("Processing documents for {}", query.tickers.join(", ")),
    ));

    // Create futures for both processing tasks
    let mut futures = Vec::new();

    // Process EDGAR filings if requested
    if query.parameters.get("filings").is_some() {
        log::debug!("Filings data is requested");
        let edgar_future = async {
            match query.to_edgar_query() {
                Ok(edgar_query) => {
                    for ticker in &edgar_query.tickers {
                        log::debug!(
                            "Fetching EDGAR filings ({}) for ticker: {} in date range {} to {}",
                            edgar_query
                                .report_types
                                .iter()
                                .map(|rt| rt.to_string())
                                .join(", "),
                            ticker,
                            edgar_query.start_date,
                            edgar_query.end_date
                        );
                        let filings =
                            filing::fetch_matching_filings(http_client, &edgar_query, progress)
                                .await?;
                        process_edgar_filings(
                            filings,
                            Arc::clone(&store),
                            Some(Arc::clone(&progress_tracker)),
                        )
                        .await?;
                    }
                    Ok::<_, anyhow::Error>(())
                }
                Err(e) => {
                    log::error!("Failed to create EDGAR query: {}", e);
                    Err(anyhow::anyhow!(e))
                }
            }
        };
        futures.push(edgar_future.boxed());
    }

    // Process earnings data if requested
    if let Ok(earnings_query) = query.to_earnings_query() {
        log::info!(
            "Fetching earnings data for ticker: {}",
            earnings_query.ticker
        );
        let ticker = earnings_query.ticker.clone();
        let start_date = earnings_query.start_date;
        let end_date = earnings_query.end_date;
        let store = Arc::clone(&store);
        let progress_tracker = progress_tracker.clone();

        let earnings_future = async move {
            let transcripts =
                earnings::fetch_transcripts(http_client, &ticker, start_date, end_date).await?;
            process_earnings_transcripts(transcripts, store, Some(progress_tracker)).await?;
            Ok::<_, anyhow::Error>(())
        };
        futures.push(earnings_future.boxed());
    }

    // Wait for all futures to complete
    futures::future::try_join_all(futures).await?;

    // Clear all progress bars at the end
    if let Some(mp) = progress {
        mp.clear()
            .map_err(|e| anyhow!("Failed to clear progress bars: {}", e))?;
    }
    Ok(())
}

fn count_tokens(doc: &Document) -> usize {
    doc.page_content.to_string().split_whitespace().count() * 4
}

/// Build context for LLM from relevant documents
///
/// Logical steps:
/// 1. Initialize empty vectors for required and all documents
/// 2. If filings requested:
///    - Create filter for doc_type and symbol
///    - Perform similarity search for each ticker
///    - Add matching docs to collection
/// 3. If earnings requested:
///    - Extract year from start date
///    - Perform similarity search with earnings filters
///    - Add matching docs to required docs
/// 4. Filter docs to match conversation tickers
/// 5. Filter chunks based on conversation tracking
/// 6. Calculate total tokens from documents
/// 7. If over token limit:
///    - Perform new similarity search with reduced scope
/// 8. Build metadata summary of final document set
/// 9. Format documents for LLM context
/// 10. Return formatted context string
async fn build_document_context(
    query: &Query,
    input: &str,
    store: Arc<Store>,
    conversation: &Conversation,
    conversation_manager: Arc<RwLock<ConversationManager>>,
) -> Result<String> {
    // 1. Get all documents specified by the query
    let mut required_docs = Vec::new();
    let all_docs = Vec::new();

    if let Some(filings) = query.parameters.get("filings") {
        if let Some(_types) = filings.get("report_types").and_then(|t| t.as_array()) {
            let mut all_docs = Vec::new();
            let filter = And(vec![
                Eq(
                    JsonField(vec!["doc_type".to_string()]),
                    LitStr("edgar_filing".to_string()),
                ),
                In(
                    JsonField(vec!["symbol".to_string()]),
                    conversation.tickers.clone(),
                ),
            ]);

            log::info!("Using filter for similarity search: {:?}", filter);
            let docs = store
                .similarity_search(
                    input,
                    20,
                    &langchain_rust::vectorstore::VecStoreOptions {
                        filters: Some(filter),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|e| anyhow!("Failed to retrieve documents from vector store: {}", e))?;

            all_docs.extend(docs);
        }
    }

    if let Some(earnings) = query.parameters.get("earnings") {
        let start_date = earnings
            .get("start_date")
            .and_then(|d| d.as_str())
            .ok_or_else(|| anyhow!("Missing earnings start_date"))?;
        let start_year = start_date.split("-").next().unwrap();
        let filter = And(vec![
            Eq(
                JsonField(["doc_type".to_string()].to_vec()),
                LitStr("earnings_transcript".to_string()),
            ),
            Eq(
                JsonField(vec!["year".to_string()]),
                LitStr(start_year.to_string()),
            ),
            In(
                JsonField(vec!["symbol".to_string()]),
                conversation.tickers.clone(),
            ),
        ]);
        let docs = store
            .similarity_search(
                input,
                20,
                &langchain_rust::vectorstore::VecStoreOptions {
                    filters: Some(filter),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| anyhow!("Failed to search documents: {}", e))?;
        required_docs.extend(docs);
    }

    // Filter docs to only include those matching conversation tickers
    let filtered_docs: Vec<_> = all_docs
        .into_iter()
        .filter(|doc: &langchain_rust::schemas::Document| {
            if let Some(symbol) = doc.metadata.get("symbol").and_then(|v| v.as_str()) {
                conversation.tickers.contains(&symbol.to_string())
            } else {
                false
            }
        })
        .collect();

    // mutate and filter required_docs based on chunks tracked in the database
    filter_chunks(
        &filtered_docs,
        conversation_manager,
        conversation,
        &mut required_docs,
    )
    .await?;

    log::debug!(
        "Query-based search returned {} documents",
        required_docs.len()
    );
    if required_docs.is_empty() {
        return Err(anyhow!("No relevant documents found in vector store"));
    }

    // 2. Calculate total tokens
    const MAX_TOKENS: usize = 130000; // FIXME Adjust based on your model
    let total_tokens: usize = required_docs
        .iter()
        .map(count_tokens) // Estimate 4 tokens per word
        .sum();

    log::info!(
        "Retrieved {} documents with approximately {} tokens",
        required_docs.len(),
        total_tokens
    );

    // 3. If we're over the token limit, use similarity search to reduce content
    let final_docs = if total_tokens > MAX_TOKENS {
        log::info!(
            "Token count ({}) exceeds limit ({}), dropping documents with lesser scores",
            total_tokens,
            MAX_TOKENS
        );
        required_docs
            .iter()
            .sorted_by(|a, b| a.score.total_cmp(&b.score))
            .scan(0, |total_tokens, doc| {
                let doc_tokens = count_tokens(doc);
                if *total_tokens + doc_tokens <= MAX_TOKENS {
                    *total_tokens += doc_tokens;
                    Some(doc.clone())
                } else {
                    None
                }
            })
            .collect()
    } else {
        required_docs
    };

    // Compile metadata summary
    let mut filing_types = std::collections::HashSet::new();
    let mut companies = std::collections::HashSet::new();
    let mut date_range = (None, None);

    for doc in &final_docs {
        if let Some(doc_type) = doc.metadata.get("type").and_then(|v| v.as_str()) {
            match doc_type {
                "edgar_filing" => {
                    if let Some(filing_type) =
                        doc.metadata.get("filing_type").and_then(|v| v.as_str())
                    {
                        filing_types.insert(filing_type.to_string());
                    }
                    if let Some(cik) = doc.metadata.get("cik").and_then(|v| v.as_str()) {
                        companies.insert(cik.to_string());
                    }
                }
                "earnings_transcript" => {
                    if let Some(symbol) = doc.metadata.get("symbol").and_then(|v| v.as_str()) {
                        companies.insert(symbol.to_string());
                    }
                }
                _ => {}
            }
        }

        // Track date range across all documents
        if let Some(date) = doc
            .metadata
            .get("filing_date")
            .or_else(|| doc.metadata.get("date"))
            .and_then(|v| v.as_str())
        {
            if let Ok(parsed_date) = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") {
                match date_range {
                    (None, None) => date_range = (Some(parsed_date), Some(parsed_date)),
                    (Some(start), Some(end)) => {
                        if parsed_date < start {
                            date_range.0 = Some(parsed_date);
                        }
                        if parsed_date > end {
                            date_range.1 = Some(parsed_date);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let summary = build_metadata_summary(filing_types, companies, date_range, &final_docs);

    // Format documents for LLM context
    let context = final_docs
        .iter()
        .map(|doc| {
            log::debug!(
                "Document (score: {:.3}):\nMetadata: {:?}\nContent: {:.100}",
                doc.score,
                doc.metadata,
                doc.page_content
            );

            let doc_type = doc
                .metadata
                .get("doc_type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("incorrect doc_type in metadata"));
            let symbol = doc.metadata.get("symbol").and_then(|v| v.as_str());

            // Format document header based on type
            let doc_header = match doc_type.and_then(DocType::from_str) {
                Ok(DocType::EdgarFiling) => {
                    format!(
                        "[{} {} Filing - {} - Score: {:.3}]",
                        symbol.unwrap(),
                        doc.metadata
                            .get("filing_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown"),
                        doc.metadata
                            .get("filing_date")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown"),
                        doc.score
                    )
                }
                // Earnings transcript
                Ok(DocType::EarningTranscript) => {
                    format!(
                        "[{} Q{} {} Earnings Call - Score: {:.3}]",
                        symbol.unwrap(),
                        doc.metadata
                            .get("quarter")
                            .and_then(|v| v.as_u64())
                            .unwrap(),
                        doc.metadata.get("year").and_then(|v| v.as_u64()).unwrap(),
                        doc.score
                    )
                }
                // Default case
                Err(e) => {
                    log::info!("Metadata loading warning: {:?}", e);
                    format!("[Document - Score: {:.3}]", doc.score)
                }
            };

            format!("{}\n{}", doc_header, doc.page_content)
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    log::info!(
        "=== Complete LLM Context ===\n{}\n=== End Context ===",
        summary
    );

    Ok(context)
}

/// Filter document chunks and track them in conversation
///
/// Logical steps:
/// 1. For each document:
///    - Extract chunk ID from metadata
///    - Get latest message ID for conversation
///    - Add chunk tracking for the message
///    - Add document to required docs collection
/// 2. Return success or error result
async fn filter_chunks(
    all_docs: &[langchain_rust::schemas::Document],
    conversation_manager: Arc<RwLock<ConversationManager>>,
    conversation: &Conversation,
    required_docs: &mut Vec<langchain_rust::schemas::Document>,
) -> Result<()> {
    for doc in all_docs.iter().cloned() {
        let chunk_id = format!("{:?}", doc.metadata);

        // Get the latest message ID for this conversation
        let messages = conversation_manager
            .read()
            .await
            .get_conversation_messages(&conversation.id, 1)
            .await?;

        if let Some(last_message) = messages.first() {
            let message_id = Uuid::parse_str(&last_message.id)?;
            // Add chunk tracking for this message
            conversation_manager
                .write()
                .await
                .add_message_chunk(&message_id, &chunk_id)
                .await?;
        }

        required_docs.push(doc);
    }
    Ok(())
}

/// Build a summary of document metadata
///
/// Logical steps:
/// 1. Initialize summary string with header
/// 2. Add filing types if present
/// 3. Add company list if present
/// 4. Add date range if present
/// 5. Group documents by source
/// 6. For each document:
///    - Extract metadata
///    - Add to appropriate summary group
/// 7. Format final summary with document counts
/// 8. Return formatted summary string
fn build_metadata_summary(
    filing_types: HashSet<String>,
    companies: HashSet<String>,
    date_range: (Option<NaiveDate>, Option<NaiveDate>),
    similar_docs: &Vec<langchain_rust::schemas::Document>,
) -> String {
    // Build metadata summary
    let mut summary = String::new();
    summary.push_str("Context includes:\n");
    if !filing_types.is_empty() {
        summary.push_str(&format!(
            "- Filing types: {}\n",
            filing_types.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    if !companies.is_empty() {
        summary.push_str(&format!(
            "- Companies: {}\n",
            companies.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    if let (Some(start), Some(end)) = date_range {
        summary.push_str(&format!("- Date range: {} to {}\n", start, end));
    }

    // Group documents by source and count chunks
    let mut doc_summaries = std::collections::HashMap::new();
    for doc in similar_docs {
        // Log full metadata for debugging
        log::debug!("Processing document with metadata: {:?}", doc.metadata);
        let key = match (
            doc.metadata.get("doc_type").and_then(|v| v.as_str()),
            doc.metadata.get("report_type").and_then(|v| v.as_str()), // Changed from filing_type
            doc.metadata.get("quarter").and_then(|v| v.as_u64()),
            doc.metadata.get("year").and_then(|v| v.as_u64()),
            doc.metadata.get("total_chunks").and_then(|v| v.as_u64()),
            doc.metadata.get("symbol").and_then(|v| v.as_str()),
            doc.metadata.get("filing_date").and_then(|v| v.as_str()),
        ) {
            (Some("edgar_filing"), Some(filing_type), _, _, total, Some(symbol), Some(date)) => {
                format!(
                    "{} {} Filing {} ({} chunks)",
                    symbol,
                    filing_type,
                    date,
                    total.unwrap_or(1)
                )
            }
            (Some("earnings_transcript"), _, Some(quarter), Some(year), total, Some(symbol), _) => {
                format!(
                    "{} Q{} {} Earnings Call ({} chunks)",
                    symbol,
                    quarter,
                    year,
                    total.unwrap_or(1)
                )
            }
            _un => {
                // More detailed logging for unknown document types
                log::warn!("Unhandled document type. Metadata: {:?}", doc.metadata);
                let doc_type = doc
                    .metadata
                    .get("doc_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let symbol = doc
                    .metadata
                    .get("symbol")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                format!(
                    "{} Document for {} (metadata: {:?})",
                    doc_type, symbol, doc.metadata
                )
            }
        };

        let entry = doc_summaries.entry(key).or_insert((0, 0));
        entry.0 += 1; // Increment chunk count
        if let Some(total) = doc.metadata.get("total_chunks").and_then(|v| v.as_i64()) {
            entry.1 = total as usize; // Update total chunks
        }
    }

    summary.push_str("\nDocuments retrieved:\n");
    for (doc_type, (chunks_found, total_chunks)) in doc_summaries {
        summary.push_str(&format!(
            "- {}: {} of {} chunks\n",
            doc_type, chunks_found, total_chunks
        ));
    }

    summary
}

async fn generate_query(
    llm: &OpenAI<OpenAIConfig>,
    input: &str,
    conversation: &Conversation,
) -> Result<(Query, String)> {
    let context = format!(
        "Current conversation context: {}\nFocus companies: {}\n\nUser question: {}",
        conversation.summary,
        conversation.tickers.join(", "),
        input
    );

    let summary = get_conversation_summary(llm, &context).await?;
    log::info!("Summary done: {}", summary);

    let query = extract_query_params(llm, input).await?;
    log::info!("Query params done: {:?}", query);

    Ok((query, summary))
}

async fn log_request(prompt: &str) -> Result<()> {
    let debug_content = serde_json::json!({
        "model": "gpt-4o-mini",
        "messages": [
            {
                "role": "system",
                "content": "You are a helpful financial analyst assistant..."
            },
            {
                "role": "user",
                "content": prompt
            }
        ],
        "stream": true
    });

    fs::write(
        "debug_last_request.json",
        serde_json::to_string_pretty(&debug_content)?,
    )?;

    Ok(())
}

async fn generate_response(
    conversation_id: Option<Uuid>,
    llm: &OpenAI<OpenAIConfig>,
    input: &str,
    context: &str,
    pg_pool: &Pool<Postgres>,
) -> Result<
    futures::stream::BoxStream<'static, Result<String, Box<dyn std::error::Error + Send + Sync>>>,
> {
    // Log input sizes
    log::info!(
        "Generating response for conversation {}: input length={}, context length={}",
        conversation_id.unwrap_or_else(Uuid::nil),
        input.len(),
        context.len()
    );

    let prompt = format!(
        "Based on the conversation context and available documents, answer this question: {}\n\n\
         Context:\n{}\n\n\
         If you don't know the answer, just say that you don't know, don't try to make up an answer. \
         Use the conversation history to provide more relevant and contextual answers. \
         Helpful answer:\n",
        input, context
    );

    // Use tokenizer to get accurate token count
    let token_count = TokenUsage::count_tokens(&prompt);
    log::info!(
        "Generated prompt length: {} chars, {} tokens",
        prompt.len(),
        token_count
    );

    // Check if we're likely to exceed OpenAI limits
    if token_count > 16000 {
        // Conservative limit for GPT-4
        log::warn!("Token count ({}) may exceed model limits", token_count);
    }

    let prompt_args = prompt_args![
        "input" => [
            "You are a helpful financial analyst assistant. Provide clear, quantitative, \
             and informative answers based on the conversation context and provided documents. \
             Maintain continuity with previous discussion points when relevant.",
            &prompt
        ]
    ];

    let conversation_id = conversation_id.unwrap_or_else(Uuid::nil);
    let memory = DatabaseMemory::new(pg_pool.clone(), conversation_id);
    let stream_chain = ConversationalChainBuilder::new()
        .llm((*llm).clone())
        .memory(Arc::new(tokio::sync::Mutex::new(memory)))
        .build()?;

    // Attempt to make the streaming request
    // Log the full request to file before making the API call
    if let Err(e) = log_request(&prompt).await {
        log::warn!("Failed to log request debug info: {}", e);
    }

    match stream_chain.stream(prompt_args.clone()).await {
        Ok(stream) => {
            log::info!("Successfully initiated OpenAI stream");
            log::debug!("Full request details saved to debug_last_request.json");

            Ok(Box::pin(stream.map(|r| match r {
                Ok(s) => {
                    log::debug!(
                        "Received chunk (truncated): {}",
                        s.content.chars().take(1000).collect::<String>()
                    );
                    Ok(s.content)
                }
                Err(e) => {
                    log::error!(
                        "Error in stream (truncated): {}",
                        e.to_string().chars().take(1000).collect::<String>()
                    );
                    Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
                }
            })))
        }
        Err(e) => {
            log::error!("Failed to create stream: {}", e);
            // Log additional error details if available
            if let Some(source) = e.source() {
                log::error!("Error source: {}", source);
            }
            Err(anyhow::anyhow!("Failed to create stream: {}", e))
        }
    }
}

async fn process_edgar_filings(
    filings: HashMap<String, filing::Filing>,
    store: Arc<Store>,
    progress_tracker: Option<Arc<ProgressTracker>>,
) -> Result<()> {
    let mut success_count = 0;
    let mut error_count = 0;
    let mut handles: Vec<tokio::task::JoinHandle<Result<(), anyhow::Error>>> = Vec::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<(), anyhow::Error>>(100);

    // Launch tasks concurrently
    for (filepath, filing) in filings {
        let tx = tx.clone();
        let store = store.clone();
        let mut progress_tracker = progress_tracker.clone();
        if let Some(tracker) = progress_tracker.as_ref() {
            let task_tracker = Arc::new(ProgressTracker::new(
                tracker.multi_progress.as_ref(),
                &format!(
                    "{} {}",
                    filing.report_type,
                    filing.filing_date.format("%Y-%m-%d")
                ),
            ));
            task_tracker.start_progress(
                100,
                &format!(
                    "Downloading [Filing {} {}]",
                    filing.report_type,
                    filing.filing_date.format("%Y-%m-%d")
                ),
            );
            progress_tracker = Some(task_tracker);
        }

        let handle = tokio::spawn(async move {
            let task_tracker = Arc::new(ProgressTracker::new(
                progress_tracker
                    .as_ref()
                    .and_then(|t| t.multi_progress.as_ref()),
                &format!(
                    "Filing {} {}",
                    filing.report_type,
                    filing.filing_date.format("%Y-%m-%d")
                ),
            ));
            task_tracker.start_progress(100, "Processing filing");

            match filing::extract_complete_submission_filing(
                &filepath,
                filing.report_type,
                store,
                Some(task_tracker),
            )
            .await
            {
                Ok(()) => {
                    let _ = tx.send(Ok(())).await;
                    Ok(())
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    let err = Err(e);
                    let _ = tx.send(Err(anyhow::anyhow!("{}", err_msg))).await;
                    err
                }
            }
        });
        handles.push(handle);
    }

    // Drop the sender to signal no more messages will be sent
    drop(tx);

    // Collect and process results
    while let Some(result) = rx.recv().await {
        match result {
            Ok(_) => success_count += 1,
            Err(e) => {
                error_count += 1;
                log::error!("Error processing filing: {}", e);
            }
        }
    }

    // Wait for all tasks
    for handle in handles {
        handle.await??;
    }

    log::info!(
        "Processed {} filings: {} successful, {} failed",
        success_count + error_count,
        success_count,
        error_count
    );

    Ok(())
}

async fn get_conversation_summary(llm: &OpenAI<OpenAIConfig>, input: &str) -> Result<String> {
    let summary_task = format!(
        "Provide a 2-3 word summary of thiass query, mentioning any ticker symbols if present. Examples:\n\
         Input: Show me Apple's revenue breakdown for Q1 2024 -> AAPL Revenue\n\
         Input: What were the key risks mentioned in the latest 10-K of TSLA? -> TSLA Risk Factors\n\
         Input: Compare Microsoft and Google cloud revenue growth -> MSFT GOOGL comparison\n\n\
         Query to summarize: {}", 
        input
    );

    // Create a new chain for this non-streaming operation
    let chain = ConversationalChainBuilder::new().llm(llm.clone()).build()?;

    match chain.invoke(prompt_args! {"input" => summary_task}).await {
        Ok(result) => Ok(result.to_string()),
        Err(e) => Err(anyhow!("Error getting summary: {:?}", e)),
    }
}

async fn extract_query_params(llm: &OpenAI<OpenAIConfig>, input: &str) -> Result<Query> {
    log::debug!("Starting extract_query_params with input: {}", input);
    let now = chrono::Local::now();
    let _today_year = now.format("%Y");
    let _today_month = now.format("%M");
    let _today_day = now.format("%d");
    let task = format!(
        r#"Extract query parameters from the input text to build a comprehensive financial analysis query.
    
    Format the parameters as a JSON object with these fields:
    - 'tickers': array of company ticker symbols
    - 'is_adr': boolean indicating if the security is an ADR (American Depositary Receipt)
    - 'parameters': object containing query parameters:
        - 'filings': optional object for SEC filings:
            - 'start_date': ISO date (YYYY-MM-DD)
            - 'end_date': ISO date (YYYY-MM-DD) 
            - 'report_types': array of SEC filing types:
                - Required reports (10-K, 10-Q for US stocks, 20-F, 6-K, 8-K for ADRs)
                - Management discussion (8-K items 2.02, 7.01, 8.01)
                - Strategic changes (8-K items 1.01, 1.02, 2.01)
                - Guidance & projections (8-K item 7.01)
                - Proxy statements (DEF 14A)
                Possible values are: {} etc, use appropriate EDGAR report types even if not mentioned here.

    Examples:
    {{"tickers": ["AAPL"], "is_adr": false, "parameters": {{"filings": {{"start_date": "2024-01-01", "end_date": "2024-03-31", "report_types": ["10-K", "10-Q", "8-K"]}}, "earnings": {{"start_date": "2024-01-01", "end_date": "2024-03-31"}} }} }}
    {{"tickers": ["BABA"], "is_adr": true, "parameters": {{"filings": {{"start_date": "2024-01-01", "end_date": "2024-03-31", "report_types": ["20-F", "6-K"]}}, "earnings": {{"start_date": "2024-01-01", "end_date": "2024-03-31"}} }} }}

    Infer which data sources to query based on the user's question:
    - Use the ticker and the company name to establish if it's a US stock or ADRs
    - Include 'filings' for questions about financial reports, SEC filings, corporate actions
    - Include 'earnings' for questions about earnings calls, management commentary, guidance
    - Include both when the question spans multiple areas
    
    Use these defaults if values are missing:
    - Latest report: date range from 'today - 90 days' to 'today'
    - Latest quarterly report: include 10-Q, 8-F for US stocks. 20-F, 40-F, 6-K filings for ADRs. If no sure ask for both.
    - Yearly reports include: 10-K for US stocks, and 20-K for ADRs. If not sure ask for both.
    - Earnings analysis: automatically include earnings call transcripts and quarterly reports (10-Q for US stocks, 6-K for ADRs) or yearly reports(10-K for US stocks, 20-F, 40-F for ADRs) depending on the contex timeline.
    
    Current date is: {}.
    Return only a json document, as it's meant to be parsed by the software. No markdown formatting is allowed. No JSON formatting is allowed including pretty-printing and newlines.
    
    Parse this user input:
    {input}"#, *edgar::report::REPORT_TYPES, now.format("%Y-%m-%d")
    )
    .to_string();

    log::info!("Task: {task}");

    // We can also guide it's response with a prompt template. Prompt templates are used to convert raw user input to a better input to the LLM.
    // Create a new chain for this non-streaming operation
    let chain = ConversationalChainBuilder::new().llm(llm.clone()).build()?;

    match chain.invoke(prompt_args! {"input" => task.clone()}).await {
        Ok(result) => {
            let result = result.to_string();
            log::debug!("Result: {:?}", result);
            let query: Query = match serde_json::from_str(&result) {
                Ok(query) => query,
                Err(e) => {
                    return Err(anyhow!("LLM returned a malformed query, halting: {}", e));
                }
            };
            log::debug!("Parsed generated query: {:?}", query);
            Ok(query)
        }
        Err(e) => Err(anyhow!("Error invoking LLMChain: {:?}", e)),
    }
}

async fn process_earnings_transcripts(
    transcripts: Vec<(earnings::Transcript, PathBuf)>,
    store: Arc<Store>,
    progress_tracker: Option<Arc<ProgressTracker>>,
) -> Result<()> {
    log::info!("Processing earnings transcripts..");
    // Create tasks with progress bars
    let mut success_count = 0;
    let mut error_count = 0;
    let mut handles: Vec<tokio::task::JoinHandle<Result<(), anyhow::Error>>> = Vec::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<(), anyhow::Error>>(100);

    // Launch tasks concurrently
    for (transcript, filepath) in transcripts {
        let tx = tx.clone();
        let store = store.clone();
        let task_tracker = progress_tracker.as_ref().map(|tracker| {
            Arc::new(ProgressTracker::new(
                tracker.multi_progress.as_ref(),
                &format!(
                    "Earnings {} Q{} {}",
                    transcript.symbol, transcript.quarter, transcript.year
                ),
            ))
        });

        if let Some(ref tracker) = task_tracker {
            tracker.start_progress(100, "Processing transcript");
        }

        let handle = tokio::spawn(async move {
            // Create metadata directly as HashMap
            let mut metadata = std::collections::HashMap::new();
            metadata.insert(
                "doc_type".to_string(),
                serde_json::Value::String("earnings_transcript".to_string()),
            );
            metadata.insert(
                "filepath".to_string(),
                serde_json::Value::String(filepath.to_string_lossy().to_string()),
            );
            metadata.insert(
                "symbol".to_string(),
                serde_json::Value::String(transcript.symbol.clone()),
            );
            metadata.insert(
                "year".to_string(),
                serde_json::Value::Number(serde_json::Number::from(transcript.year)),
            );
            metadata.insert(
                "quarter".to_string(),
                serde_json::Value::Number(serde_json::Number::from(transcript.quarter)),
            );
            metadata.insert(
                "chunk_index".to_string(),
                serde_json::Value::Number(serde_json::Number::from(0)),
            );
            metadata.insert(
                "total_chunks".to_string(),
                serde_json::Value::Number(serde_json::Number::from(1)),
            );

            let content = transcript.content.clone();
            crate::vectorstore::store_document(content, metadata, store.as_ref()).await?;

            let _ = tx.send(Ok(())).await;
            Ok::<_, anyhow::Error>(())
        });
        handles.push(handle);
    }

    // Drop sender to signal no more messages
    drop(tx);

    // Collect results from channel
    while let Some(result) = rx.recv().await {
        match result {
            Ok(_) => success_count += 1,
            Err(e) => {
                error_count += 1;
                log::error!("Error processing transcript: {}", e);
            }
        }
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await??;
    }

    log::info!(
        "Processed {} transcripts: {} successful, {} failed",
        success_count + error_count,
        success_count,
        error_count
    );

    Ok(())
}

pub async fn eval(
    input: &str,
    conversation: &Conversation,
    http_client: &reqwest::Client,
    llm: &OpenAI<OpenAIConfig>,
    store: Arc<Store>,
    conversation_manager: Arc<RwLock<ConversationManager>>,
    pg_pool: Pool<Postgres>,
) -> Result<(
    futures::stream::BoxStream<'static, Result<String, Box<dyn std::error::Error + Send + Sync>>>,
    String,
)> {
    // Store user message
    conversation_manager
        .write()
        .await
        .add_message(
            &conversation.id,
            MessageRole::User,
            input.to_string(),
            serde_json::json!({
                "type": "question"
            }),
        )
        .await?;

    // Generate response
    let (query, summary) = generate_query(llm, input, conversation).await?;

    // Update conversation tickers if new ones are found
    let existing_tickers: HashSet<_> = conversation.tickers.iter().cloned().collect();
    let new_tickers: HashSet<_> = query.tickers.iter().cloned().collect();

    if !new_tickers.is_subset(&existing_tickers) {
        let updated_tickers: Vec<_> = existing_tickers.union(&new_tickers).cloned().collect();
        conversation_manager
            .write()
            .await
            .update_tickers(&conversation.id, updated_tickers.clone())
            .await?;
        log::info!("Updated conversation tickers: {:?}", updated_tickers);
    }

    // Initiate multi progress bars for the CLI app
    let multi_progress = if std::io::stdout().is_terminal() {
        let mp = MultiProgress::new();
        mp.set_move_cursor(true);
        Some(Arc::new(mp))
    } else {
        None
    };

    let store = Arc::new(store);
    process_documents(
        &query,
        http_client,
        Arc::clone(&store),
        multi_progress.as_ref(),
    )
    .await?;
    log::info!(
        "Starting response generation for conversation {}",
        conversation.id
    );

    let context = build_document_context(
        &query,
        input,
        Arc::clone(&store),
        conversation,
        Arc::clone(&conversation_manager),
    )
    .await?;

    log::debug!(
        "Context preparation complete. Context size: {} bytes",
        context.len()
    );

    log::info!("Initiating response stream");
    let stream =
        match generate_response(Some(conversation.id), llm, input, &context, &pg_pool).await {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to generate response: {}", e);
                return Err(anyhow::anyhow!("Failed to generate response: {}", e));
            }
        };

    // Create a new channel for collecting the complete response
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(32);

    // Clone necessary values for the spawned task
    let conversation_id = conversation.id;
    let query_clone = query.clone();
    let summary_clone = summary.clone();
    let conversation_manager_clone = Arc::clone(&conversation_manager);

    // Create a new stream that both forwards chunks and collects them
    let collected_stream = Box::pin(stream.inspect(move |result| {
        if let Ok(chunk) = result {
            let _ = tx.try_send(chunk.clone());
        }
    }));

    // Spawn task to collect and store complete response
    tokio::spawn(async move {
        let mut complete_response = String::new();
        while let Some(chunk) = rx.recv().await {
            complete_response.push_str(&chunk);
        }

        if let Err(e) = conversation_manager_clone
            .write()
            .await
            .add_message(
                &conversation_id,
                MessageRole::Assistant,
                complete_response,
                serde_json::json!({
                    "type": "answer",
                    "query": query_clone,
                    "summary": summary_clone
                }),
            )
            .await
        {
            log::error!("Failed to store assistant response: {}", e);
        }
    });

    Ok((collected_stream, summary))
}
