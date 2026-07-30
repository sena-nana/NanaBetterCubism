use super::config::bearer_matches;
use super::tools::{call_mcp_tool, list_mcp_tools, McpToolContext};
use axum::{
    body::Body,
    extract::Request,
    http::{header::AUTHORIZATION, StatusCode},
    middleware::{from_fn_with_state, Next},
    response::{IntoResponse, Response},
    Router,
};
use rmcp::{
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
    transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    ErrorData as McpError, RoleServer, ServerHandler,
};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct AuthState {
    token: Arc<Mutex<String>>,
}

#[derive(Clone)]
struct CubismMcpHandler {
    context: McpToolContext,
}

impl ServerHandler for CubismMcpHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "NanaBetterCubism MCP：经本机应用中转操作 Cubism Editor 与检查 PSD。写操作走预览→事务→结果查询。",
            )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let allow_writes = *self.context.allow_writes.lock().unwrap();
        Ok(ListToolsResult::with_all_items(list_mcp_tools(allow_writes)))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        match call_mcp_tool(&self.context, &request.name, request.arguments).await {
            Ok(result) => Ok(result.into()),
            Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(
                serde_json::json!({
                    "ok": false,
                    "error": { "code": error.code, "message": error.message }
                })
                .to_string(),
            )])
            .into()),
        }
    }
}

pub(crate) struct HttpServerTask {
    pub cancellation: CancellationToken,
    pub join: tokio::task::JoinHandle<Result<(), String>>,
}

pub(crate) async fn bind_and_serve(
    port: u16,
    token: Arc<Mutex<String>>,
    context: McpToolContext,
) -> Result<HttpServerTask, String> {
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port)))
        .await
        .map_err(|error| format!("无法绑定 127.0.0.1:{port}：{error}"))?;

    let cancellation = CancellationToken::new();
    let config = StreamableHttpServerConfig::default()
        .with_cancellation_token(cancellation.clone())
        .with_json_response(true)
        .with_allowed_hosts([
            "127.0.0.1".to_string(),
            format!("127.0.0.1:{port}"),
            "localhost".to_string(),
            format!("localhost:{port}"),
            "[::1]".to_string(),
            format!("[::1]:{port}"),
        ]);

    let service: StreamableHttpService<CubismMcpHandler, LocalSessionManager> =
        StreamableHttpService::new(
            {
                let context = context.clone();
                move || Ok(CubismMcpHandler {
                    context: context.clone(),
                })
            },
            Arc::new(LocalSessionManager::default()),
            config,
        );

    let router = Router::new()
        .nest_service("/mcp", service)
        .layer(from_fn_with_state(AuthState { token }, require_bearer));

    let join = tokio::spawn({
        let cancellation = cancellation.clone();
        async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async move { cancellation.cancelled_owned().await })
                .await
                .map_err(|error| format!("MCP Server 运行失败：{error}"))
        }
    });

    Ok(HttpServerTask { cancellation, join })
}

#[cfg(test)]
pub(crate) async fn bind_test_server(port: u16) -> Result<HttpServerTask, String> {
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port)))
        .await
        .map_err(|error| error.to_string())?;
    let cancellation = CancellationToken::new();
    let join = tokio::spawn({
        let cancellation = cancellation.clone();
        async move {
            axum::serve(listener, Router::new())
                .with_graceful_shutdown(async move { cancellation.cancelled_owned().await })
                .await
                .map_err(|error| error.to_string())
        }
    });
    Ok(HttpServerTask { cancellation, join })
}

async fn require_bearer(
    state: axum::extract::State<AuthState>,
    request: Request,
    next: Next,
) -> Response {
    let header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    let expected = state.token.lock().unwrap().clone();
    if bearer_matches(&expected, header) {
        next.run(request).await
    } else {
        (StatusCode::UNAUTHORIZED, Body::from("Unauthorized")).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;

    #[tokio::test]
    async fn rejects_requests_without_bearer_token() {
        let token = Arc::new(Mutex::new("test-token".into()));
        let router = Router::new()
            .route("/mcp", get(|| async { "ok" }))
            .layer(from_fn_with_state(AuthState { token }, require_bearer));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        let client = reqwest::Client::new();
        let url = format!("http://{addr}/mcp");
        assert_eq!(
            client.get(&url).send().await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        let allowed = client
            .get(&url)
            .header(AUTHORIZATION, "Bearer test-token")
            .send()
            .await
            .unwrap();
        assert_eq!(allowed.status(), StatusCode::OK);
        assert_eq!(allowed.text().await.unwrap(), "ok");
    }
}
