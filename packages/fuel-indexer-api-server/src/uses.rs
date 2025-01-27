use crate::api::{ApiError, ApiResult, HttpError};
use anyhow::Result;
use async_std::sync::{Arc, RwLock};
use axum::{
    body::Body,
    extract::{multipart::Multipart, Extension, Json, Path},
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use fuel_indexer_database::{
    queries,
    types::{IndexAsset, IndexAssetType},
    IndexerConnectionPool,
};
use fuel_indexer_lib::{
    config::IndexerConfig,
    utils::{
        AssetReloadRequest, FuelNodeHealthResponse, IndexStopRequest, ServiceRequest,
        ServiceStatus,
    },
};
use fuel_indexer_schema::db::{
    graphql::GraphqlQueryBuilder, manager::SchemaManager, tables::Schema,
};
use hyper::Client;
use hyper_rustls::HttpsConnectorBuilder;
use serde::Deserialize;
use serde_json::{json, Value};
use std::str::FromStr;
use std::time::Instant;
use tokio::sync::mpsc::Sender;
use tracing::error;

#[cfg(feature = "metrics")]
use fuel_indexer_metrics::{encode_metrics_response, METRICS};

#[derive(Clone, Debug, Deserialize)]
pub struct Query {
    pub query: String,
    #[allow(unused)] // TODO
    pub params: String,
}

pub(crate) async fn query_graph(
    Path(name): Path<String>,
    Json(query): Json<Query>,
    Extension(pool): Extension<IndexerConnectionPool>,
    Extension(manager): Extension<Arc<RwLock<SchemaManager>>>,
) -> ApiResult<axum::Json<Value>> {
    match manager.read().await.load_schema(&name).await {
        Ok(schema) => match run_query(query, schema, &pool).await {
            Ok(response) => Ok(axum::Json(response)),
            Err(e) => Err(e),
        },
        Err(_e) => Err(ApiError::Http(HttpError::NotFound(format!(
            "The graph '{}' was not found.",
            name
        )))),
    }
}

pub(crate) async fn get_fuel_status(config: &IndexerConfig) -> ServiceStatus {
    #[cfg(feature = "metrics")]
    METRICS.web.health.requests.inc();

    let https = HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .build();

    let client = Client::builder().build::<_, hyper::Body>(https);
    match client
        .get(config.to_owned().fuel_node.health_check_uri())
        .await
    {
        Ok(r) => {
            let body_bytes = hyper::body::to_bytes(r.into_body())
                .await
                .unwrap_or_default();

            let fuel_node_health: FuelNodeHealthResponse =
                serde_json::from_slice(&body_bytes).unwrap_or_default();

            ServiceStatus::from(fuel_node_health)
        }
        Err(e) => {
            error!("Failed to fetch fuel /health status: {}.", e);
            ServiceStatus::NotOk
        }
    }
}

pub(crate) async fn health_check(
    Extension(config): Extension<IndexerConfig>,
    Extension(pool): Extension<IndexerConnectionPool>,
    Extension(start_time): Extension<Arc<Instant>>,
) -> ApiResult<axum::Json<Value>> {
    let db_status = pool.is_connected().await.unwrap_or(ServiceStatus::NotOk);
    let uptime = start_time.elapsed().as_secs().to_string();
    let fuel_core_status = get_fuel_status(&config).await;

    Ok(Json(json!({
        "fuel_core_status": fuel_core_status,
        "uptime(seconds)": uptime,
        "database_status": db_status,
    })))
}

async fn authenticate_user(_signature: &str) -> Option<Result<bool, ApiError>> {
    // TODO: Placeholder until actual authentication scheme is in place
    Some(Ok(true))
}

pub(crate) async fn authorize_middleware<B>(
    mut req: Request<B>,
    next: Next<B>,
) -> impl IntoResponse {
    let header = req
        .headers()
        .get(http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok());
    let auth_header = if let Some(auth_header) = header {
        auth_header
    } else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    if let Some(current_user) = authenticate_user(auth_header).await {
        req.extensions_mut().insert(current_user);
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

pub(crate) async fn stop_index(
    Path((namespace, identifier)): Path<(String, String)>,
    Extension(tx): Extension<Option<Sender<ServiceRequest>>>,
    Extension(pool): Extension<IndexerConnectionPool>,
) -> ApiResult<axum::Json<Value>> {
    let mut conn = pool.acquire().await?;

    let _ = queries::start_transaction(&mut conn).await?;

    if let Err(_e) = queries::remove_index(&mut conn, &namespace, &identifier).await {
        queries::revert_transaction(&mut conn).await?;
    } else {
        queries::commit_transaction(&mut conn).await?;
    }

    if let Some(tx) = tx {
        tx.send(ServiceRequest::IndexStop(IndexStopRequest {
            namespace,
            identifier,
        }))
        .await?;

        return Ok(Json(json!({
            "success": "true",
        })));
    }

    Err(ApiError::default())
}

pub(crate) async fn register_index_assets(
    Path((namespace, identifier)): Path<(String, String)>,
    Extension(tx): Extension<Option<Sender<ServiceRequest>>>,
    Extension(schema_manager): Extension<Arc<RwLock<SchemaManager>>>,
    Extension(pool): Extension<IndexerConnectionPool>,
    multipart: Option<Multipart>,
) -> ApiResult<axum::Json<Value>> {
    if let Some(mut multipart) = multipart {
        let mut conn = match pool.acquire().await {
            Ok(conn) => conn,
            Err(e) => {
                return Err::<axum::Json<serde_json::Value>, ApiError>(e.into());
            }
        };

        let _ = queries::start_transaction(&mut conn).await?;

        let mut assets: Vec<IndexAsset> = Vec::new();

        while let Some(field) = multipart.next_field().await.unwrap() {
            let name = field.name().unwrap_or("").to_string();
            let data = field.bytes().await.unwrap_or_default();
            let asset_type =
                IndexAssetType::from_str(&name).expect("Invalid asset type.");

            let asset: IndexAsset = match asset_type {
                IndexAssetType::Wasm | IndexAssetType::Manifest => {
                    queries::register_index_asset(
                        &mut conn,
                        &namespace,
                        &identifier,
                        data.to_vec(),
                        asset_type,
                    )
                    .await?
                }
                IndexAssetType::Schema => {
                    match queries::register_index_asset(
                        &mut conn,
                        &namespace,
                        &identifier,
                        data.to_vec(),
                        IndexAssetType::Schema,
                    )
                    .await
                    {
                        Ok(result) => {
                            schema_manager
                                .write()
                                .await
                                .new_schema(&namespace, &String::from_utf8_lossy(&data))
                                .await?;

                            result
                        }
                        Err(e) => {
                            return Err(e.into());
                        }
                    }
                }
            };

            assets.push(asset);
        }

        let _ = queries::commit_transaction(&mut conn).await?;

        if let Some(tx) = tx {
            tx.send(ServiceRequest::AssetReload(AssetReloadRequest {
                namespace,
                identifier,
            }))
            .await?;
        }

        return Ok(Json(json!({
            "success": "true",
            "assets": assets,
        })));
    }

    Err(ApiError::default())
}

pub async fn run_query(
    query: Query,
    schema: Schema,
    pool: &IndexerConnectionPool,
) -> ApiResult<Value> {
    let builder = GraphqlQueryBuilder::new(&schema, &query.query)?;
    let query = builder.build()?;

    let queries = query.as_sql(true).join(";\n");

    let mut conn = pool.acquire().await?;

    match queries::run_query(&mut conn, queries).await {
        Ok(ans) => {
            let row: Value = serde_json::from_value(ans)?;
            Ok(row)
        }
        Err(e) => {
            error!("Error querying database: {}.", e);
            Err(e.into())
        }
    }
}

pub async fn metrics(_req: Request<Body>) -> impl IntoResponse {
    #[cfg(feature = "metrics")]
    {
        match encode_metrics_response() {
            Ok((buff, fmt_type)) => Response::builder()
                .status(StatusCode::OK)
                .header(http::header::CONTENT_TYPE, &fmt_type)
                .body(Body::from(buff))
                .unwrap(),
            Err(_e) => Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("Error"))
                .unwrap(),
        }
    }
    #[cfg(not(feature = "metrics"))]
    {
        (StatusCode::NOT_FOUND, "Metrics collection disabled.")
    }
}
