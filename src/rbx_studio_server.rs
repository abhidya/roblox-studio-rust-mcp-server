use crate::error::{Report, Result};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{extract::State, Json};
use color_eyre::eyre::{eyre, Error, OptionExt};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
    },
    schemars, tool, tool_handler, tool_router, ErrorData, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::oneshot::Receiver;
use tokio::sync::{mpsc, watch, Mutex};
use tokio::time::Duration;
use uuid::Uuid;

pub const STUDIO_PLUGIN_PORT: u16 = 44755;
const LONG_POLL_DURATION: Duration = Duration::from_secs(15);

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ToolArguments {
    args: ToolArgumentValues,
    id: Option<Uuid>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct RunCommandResponse {
    success: bool,
    response: String,
    id: Uuid,
}

pub struct AppState {
    process_queue: VecDeque<ToolArguments>,
    output_map: HashMap<Uuid, mpsc::UnboundedSender<Result<String>>>,
    waiter: watch::Receiver<()>,
    trigger: watch::Sender<()>,
}
pub type PackedState = Arc<Mutex<AppState>>;

impl AppState {
    pub fn new() -> Self {
        let (trigger, waiter) = watch::channel(());
        Self {
            process_queue: VecDeque::new(),
            output_map: HashMap::new(),
            waiter,
            trigger,
        }
    }
}

impl ToolArguments {
    fn new(args: ToolArgumentValues) -> (Self, Uuid) {
        Self { args, id: None }.with_id()
    }
    fn with_id(self) -> (Self, Uuid) {
        let id = Uuid::new_v4();
        (
            Self {
                args: self.args,
                id: Some(id),
            },
            id,
        )
    }
}

fn encode_query_value(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            b' ' => vec!['+'],
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

#[derive(Clone)]
pub struct RBXStudioServer {
    state: PackedState,
    tool_router: ToolRouter<Self>,
}

#[tool_handler]
impl ServerHandler for RBXStudioServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "Roblox_Studio".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: Some("Roblox Studio MCP Server".to_string()),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "You must aware of current studio mode before using any tools, infer the mode from conversation context or get_studio_mode.
User run_code to query data from Roblox Studio place or to change it
After calling run_script_in_play_mode, the datamodel status will be reset to stop mode.
Prefer using start_stop_play tool instead run_script_in_play_mode, Only used run_script_in_play_mode to run one time unit test code on server datamodel.
"
                    .to_string(),
            ),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema, Clone)]
struct RunCode {
    #[schemars(description = "Code to run")]
    command: String,
}
#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema, Clone)]
struct InsertModel {
    #[schemars(description = "Query to search for the model")]
    query: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema, Clone)]
struct GetConsoleOutput {}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema, Clone)]
struct GetStudioMode {}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema, Clone)]
struct StartStopPlay {
    #[schemars(
        description = "Mode to start or stop, must be start_play, stop, or run_server. Don't use run_server unless you are sure no client/player is needed."
    )]
    mode: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema, Clone)]
struct RunScriptInPlayMode {
    #[schemars(description = "Code to run")]
    code: String,
    #[schemars(description = "Timeout in seconds, defaults to 100 seconds")]
    timeout: Option<u32>,
    #[schemars(description = "Mode to run in, must be start_play or run_server")]
    mode: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema, Clone)]
struct SearchAssets {
    #[schemars(description = "Search query for Creator Store assets")]
    query: String,
    #[schemars(
        description = "Maximum number of ranked results to return across all searched categories (default: 10, max: 50)"
    )]
    max_results: Option<u32>,
    #[schemars(
        description = "Optional Creator Store categories to search. Defaults to Model, MeshPart, Decal, Audio, Plugin, Video, and FontFamily."
    )]
    categories: Option<Vec<String>>,
    #[schemars(description = "When true, only return assets from verified creators.")]
    verified_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolboxSearchResponse {
    creator_store_assets: Option<Vec<ToolboxCreatorStoreAsset>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolboxCreatorStoreAsset {
    asset: Option<ToolboxAsset>,
    creator: Option<ToolboxCreator>,
    creator_store_product: Option<ToolboxProduct>,
    voting: Option<ToolboxVoting>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolboxAsset {
    id: u64,
    name: Option<String>,
    description: Option<String>,
    asset_type_id: Option<u32>,
    category_path: Option<String>,
    create_time: Option<String>,
    update_time: Option<String>,
    has_scripts: Option<bool>,
    script_count: Option<u32>,
    instance_counts: Option<ToolboxInstanceCounts>,
    object_mesh_summary: Option<ToolboxMeshSummary>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolboxCreator {
    name: Option<String>,
    verified: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolboxProduct {
    purchasable: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolboxVoting {
    vote_count: Option<u64>,
    up_vote_percent: Option<u32>,
    up_votes: Option<u64>,
    down_votes: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolboxInstanceCounts {
    script: Option<u32>,
    mesh_part: Option<u32>,
    animation: Option<u32>,
    decal: Option<u32>,
    audio: Option<u32>,
    tool: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolboxMeshSummary {
    triangles: Option<u64>,
    vertices: Option<u64>,
}

#[derive(Debug)]
struct RankedCreatorStoreAsset {
    category: String,
    id: u64,
    name: String,
    creator: String,
    verified: bool,
    purchasable: bool,
    asset_type_id: Option<u32>,
    category_path: Option<String>,
    description: Option<String>,
    create_time: Option<String>,
    update_time: Option<String>,
    has_scripts: bool,
    script_count: u32,
    mesh_parts: u32,
    animations: u32,
    decals: u32,
    audio: u32,
    tools: u32,
    triangles: Option<u64>,
    vertices: Option<u64>,
    vote_count: u64,
    up_vote_percent: u32,
    up_votes: u64,
    down_votes: u64,
    score: f64,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema, Clone)]
enum ToolArgumentValues {
    RunCode(RunCode),
    InsertModel(InsertModel),
    GetConsoleOutput(GetConsoleOutput),
    StartStopPlay(StartStopPlay),
    RunScriptInPlayMode(RunScriptInPlayMode),
    GetStudioMode(GetStudioMode),
}
#[tool_router]
impl RBXStudioServer {
    pub fn new(state: PackedState) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Runs a command in Roblox Studio and returns the printed output. Can be used to both make changes and retrieve information"
    )]
    async fn run_code(
        &self,
        Parameters(args): Parameters<RunCode>,
    ) -> Result<CallToolResult, ErrorData> {
        self.generic_tool_run(ToolArgumentValues::RunCode(args))
            .await
    }

    #[tool(
        description = "Inserts a model from the Roblox marketplace into the workspace. Returns the inserted model name."
    )]
    async fn insert_model(
        &self,
        Parameters(args): Parameters<InsertModel>,
    ) -> Result<CallToolResult, ErrorData> {
        self.generic_tool_run(ToolArgumentValues::InsertModel(args))
            .await
    }

    #[tool(description = "Get the console output from Roblox Studio.")]
    async fn get_console_output(
        &self,
        Parameters(args): Parameters<GetConsoleOutput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.generic_tool_run(ToolArgumentValues::GetConsoleOutput(args))
            .await
    }

    #[tool(
        description = "Start or stop play mode or run the server, Don't enter run_server mode unless you are sure no client/player is needed."
    )]
    async fn start_stop_play(
        &self,
        Parameters(args): Parameters<StartStopPlay>,
    ) -> Result<CallToolResult, ErrorData> {
        self.generic_tool_run(ToolArgumentValues::StartStopPlay(args))
            .await
    }

    #[tool(
        description = "Run a script in play mode and automatically stop play after script finishes or timeout. Returns the output of the script.
        Result format: { success: boolean, value: string, error: string, logs: { level: string, message: string, ts: number }[], errors: { level: string, message: string, ts: number }[], duration: number, isTimeout: boolean }.
        - Prefer using start_stop_play tool instead run_script_in_play_mode, Only used run_script_in_play_mode to run one time unit test code on server datamodel.
        - After calling run_script_in_play_mode, the datamodel status will be reset to stop mode.
        - If It returns `StudioTestService: Previous call to start play session has not been completed`, call start_stop_play tool to stop play mode first then try it again."
    )]
    async fn run_script_in_play_mode(
        &self,
        Parameters(args): Parameters<RunScriptInPlayMode>,
    ) -> Result<CallToolResult, ErrorData> {
        self.generic_tool_run(ToolArgumentValues::RunScriptInPlayMode(args))
            .await
    }

    #[tool(
        description = "Get the current studio mode. Returns the studio mode. The result will be one of start_play, run_server, or stop."
    )]
    async fn get_studio_mode(
        &self,
        Parameters(args): Parameters<GetStudioMode>,
    ) -> Result<CallToolResult, ErrorData> {
        self.generic_tool_run(ToolArgumentValues::GetStudioMode(args))
            .await
    }

    #[tool(
        description = "Searches the Roblox Creator Store across asset categories using the Toolbox Service API, not the legacy free-model-only Studio search. Returns ranked assets with category, votes, creator verification, script counts, mesh/audio/UI indicators, and IDs for agentic asset selection."
    )]
    async fn search_assets(
        &self,
        Parameters(args): Parameters<SearchAssets>,
    ) -> Result<CallToolResult, ErrorData> {
        if args.query.trim().is_empty() {
            return Ok(CallToolResult::error(vec![Content::text(
                "query must not be empty",
            )]));
        }

        match Self::search_creator_store(args).await {
            Ok(results) => Ok(CallToolResult::success(vec![Content::text(results)])),
            Err(err) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Creator Store search failed: {err}"
            ))])),
        }
    }

    async fn search_creator_store(
        args: SearchAssets,
    ) -> std::result::Result<String, reqwest::Error> {
        const DEFAULT_CATEGORIES: [&str; 7] = [
            "Model",
            "MeshPart",
            "Decal",
            "Audio",
            "Plugin",
            "Video",
            "FontFamily",
        ];

        let max_results = args.max_results.unwrap_or(10).clamp(1, 50) as usize;
        let categories: Vec<String> = args
            .categories
            .unwrap_or_else(|| {
                DEFAULT_CATEGORIES
                    .iter()
                    .map(|category| category.to_string())
                    .collect()
            })
            .into_iter()
            .filter(|category| !category.trim().is_empty())
            .collect();
        let verified_only = args.verified_only.unwrap_or(false);

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(12))
            .build()?;

        let futures = categories.into_iter().map(|category| {
            let client = client.clone();
            let query = args.query.clone();
            let page_size = max_results.clamp(1, 100).to_string();
            async move {
                let mut url = format!(
                    "https://apis.roblox.com/toolbox-service/v2/assets:search?searchCategoryType={}&query={}&maxPageSize={}&searchView=Full&sortCategory=Relevance",
                    encode_query_value(&category),
                    encode_query_value(&query),
                    page_size
                );
                if verified_only {
                    url.push_str("&includeOnlyVerifiedCreators=true");
                }

                let response = client.get(url).send().await?;
                if !response.status().is_success() {
                    return Ok::<Vec<RankedCreatorStoreAsset>, reqwest::Error>(Vec::new());
                }
                let payload = response.json::<ToolboxSearchResponse>().await?;
                let assets = payload.creator_store_assets.unwrap_or_default();

                Ok(assets
                    .into_iter()
                    .filter_map(|item| Self::rank_creator_store_asset(&category, item))
                    .collect::<Vec<_>>())
            }
        });

        let mut ranked = Vec::new();
        for result in futures::future::join_all(futures).await {
            ranked.extend(result?);
        }

        ranked.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        ranked.truncate(max_results);

        if ranked.is_empty() {
            return Ok(format!(
                "No Creator Store assets found for '{}'. Searched categories through toolbox-service/v2/assets:search.",
                args.query
            ));
        }

        let mut lines = vec![format!(
            "Found {} Creator Store assets for '{}', ranked across categories:",
            ranked.len(),
            args.query
        )];

        for (index, asset) in ranked.iter().enumerate() {
            let verified = if asset.verified { " verified" } else { "" };
            let scripts = if asset.has_scripts || asset.script_count > 0 {
                format!("scripts={}", asset.script_count)
            } else {
                "scripts=0".to_string()
            };
            let content = format!(
                "meshParts={} animations={} decals={} audio={} tools={}",
                asset.mesh_parts, asset.animations, asset.decals, asset.audio, asset.tools
            );
            let mesh = match (asset.triangles, asset.vertices) {
                (Some(triangles), Some(vertices)) => {
                    format!(" triangles={} vertices={}", triangles, vertices)
                }
                _ => String::new(),
            };
            let category_path = asset
                .category_path
                .as_deref()
                .map(|path| format!(" path={path}"))
                .unwrap_or_default();
            let preview = asset
                .description
                .as_deref()
                .map(|description| description.lines().next().unwrap_or("").trim())
                .filter(|description| !description.is_empty())
                .map(|description| {
                    if description.len() > 120 {
                        format!("{}...", &description[..117])
                    } else {
                        description.to_string()
                    }
                })
                .unwrap_or_default();

            lines.push(format!(
                "\n{}. {} (ID: {})\n   category={}{} assetTypeId={:?} creator={}{} purchasable={}\n   score={:.1} votes={} upVotePercent={} up={} down={} {}\n   {}{}{}\n   created={:?} updated={:?}\n   {}",
                index + 1,
                asset.name,
                asset.id,
                asset.category,
                category_path,
                asset.asset_type_id,
                asset.creator,
                verified,
                asset.purchasable,
                asset.score,
                asset.vote_count,
                asset.up_vote_percent,
                asset.up_votes,
                asset.down_votes,
                scripts,
                content,
                mesh,
                if asset.has_scripts { " SCRIPT_REVIEW_REQUIRED" } else { "" },
                asset.create_time,
                asset.update_time,
                preview
            ));
        }

        lines.push("\nSource: Roblox Creator Store Toolbox Service v2. This intentionally replaces InsertService:GetFreeModels search, which only covered legacy free-model results.".to_string());
        Ok(lines.join("\n"))
    }

    fn rank_creator_store_asset(
        category: &str,
        item: ToolboxCreatorStoreAsset,
    ) -> Option<RankedCreatorStoreAsset> {
        let asset = item.asset?;
        let creator = item.creator;
        let product = item.creator_store_product;
        let voting = item.voting;
        let instance_counts = asset.instance_counts;
        let mesh_summary = asset.object_mesh_summary;

        let verified = creator
            .as_ref()
            .and_then(|value| value.verified)
            .unwrap_or(false);
        let creator_name = creator
            .and_then(|value| value.name)
            .unwrap_or_else(|| "Unknown".to_string());
        let purchasable = product.and_then(|value| value.purchasable).unwrap_or(false);
        let vote_count = voting
            .as_ref()
            .and_then(|value| value.vote_count)
            .unwrap_or(0);
        let up_vote_percent = voting
            .as_ref()
            .and_then(|value| value.up_vote_percent)
            .unwrap_or(0);
        let up_votes = voting
            .as_ref()
            .and_then(|value| value.up_votes)
            .unwrap_or(0);
        let down_votes = voting.and_then(|value| value.down_votes).unwrap_or(0);

        let script_count = instance_counts
            .as_ref()
            .and_then(|value| value.script)
            .or(asset.script_count)
            .unwrap_or(0);
        let mesh_parts = instance_counts
            .as_ref()
            .and_then(|value| value.mesh_part)
            .unwrap_or(0);
        let animations = instance_counts
            .as_ref()
            .and_then(|value| value.animation)
            .unwrap_or(0);
        let decals = instance_counts
            .as_ref()
            .and_then(|value| value.decal)
            .unwrap_or(0);
        let audio = instance_counts
            .as_ref()
            .and_then(|value| value.audio)
            .unwrap_or(0);
        let tools = instance_counts
            .as_ref()
            .and_then(|value| value.tool)
            .unwrap_or(0);
        let has_scripts = asset.has_scripts.unwrap_or(false) || script_count > 0;

        let mut score = 0.0;
        if vote_count > 0 {
            score += (vote_count as f64).ln() * 10.0;
        }
        score += up_vote_percent as f64 * 0.45;
        if verified {
            score += 20.0;
        }
        if purchasable {
            score += 5.0;
        }
        if asset
            .description
            .as_deref()
            .map(|value| value.len())
            .unwrap_or(0)
            > 20
        {
            score += 5.0;
        }
        if has_scripts {
            score -= 8.0;
        }

        Some(RankedCreatorStoreAsset {
            category: category.to_string(),
            id: asset.id,
            name: asset.name.unwrap_or_else(|| "Untitled".to_string()),
            creator: creator_name,
            verified,
            purchasable,
            asset_type_id: asset.asset_type_id,
            category_path: asset.category_path,
            description: asset.description,
            create_time: asset.create_time,
            update_time: asset.update_time,
            has_scripts,
            script_count,
            mesh_parts,
            animations,
            decals,
            audio,
            tools,
            triangles: mesh_summary.as_ref().and_then(|value| value.triangles),
            vertices: mesh_summary.and_then(|value| value.vertices),
            vote_count,
            up_vote_percent,
            up_votes,
            down_votes,
            score,
        })
    }

    async fn generic_tool_run(
        &self,
        args: ToolArgumentValues,
    ) -> Result<CallToolResult, ErrorData> {
        let (command, id) = ToolArguments::new(args);
        tracing::debug!("Running command: {:?}", command);
        let (tx, mut rx) = mpsc::unbounded_channel::<Result<String>>();
        let trigger = {
            let mut state = self.state.lock().await;
            state.process_queue.push_back(command);
            state.output_map.insert(id, tx);
            state.trigger.clone()
        };
        trigger
            .send(())
            .map_err(|e| ErrorData::internal_error(format!("Unable to trigger send {e}"), None))?;
        let result = rx
            .recv()
            .await
            .ok_or(ErrorData::internal_error("Couldn't receive response", None))?;
        {
            let mut state = self.state.lock().await;
            state.output_map.remove_entry(&id);
        }
        tracing::debug!("Sending to MCP: {result:?}");
        match result {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(err) => Ok(CallToolResult::error(vec![Content::text(err.to_string())])),
        }
    }
}

pub async fn request_handler(State(state): State<PackedState>) -> Result<impl IntoResponse> {
    let timeout = tokio::time::timeout(LONG_POLL_DURATION, async {
        let mut waiter = { state.lock().await.waiter.clone() };
        loop {
            {
                let mut state = state.lock().await;
                if let Some(task) = state.process_queue.pop_front() {
                    return Ok::<ToolArguments, Error>(task);
                }
            }
            waiter.changed().await?
        }
    })
    .await;
    match timeout {
        Ok(result) => Ok(Json(result?).into_response()),
        _ => Ok((StatusCode::LOCKED, String::new()).into_response()),
    }
}

pub async fn response_handler(
    State(state): State<PackedState>,
    Json(payload): Json<RunCommandResponse>,
) -> Result<impl IntoResponse> {
    tracing::debug!("Received reply from studio {payload:?}");
    let mut state = state.lock().await;
    let tx = state
        .output_map
        .remove(&payload.id)
        .ok_or_eyre("Unknown ID")?;
    let result: Result<String, Report> = if payload.success {
        Ok(payload.response)
    } else {
        Err(Report::from(eyre!(payload.response)))
    };
    Ok(tx.send(result)?)
}

pub async fn proxy_handler(
    State(state): State<PackedState>,
    Json(command): Json<ToolArguments>,
) -> Result<impl IntoResponse> {
    let id = command.id.ok_or_eyre("Got proxy command with no id")?;
    tracing::debug!("Received request to proxy {command:?}");
    let (tx, mut rx) = mpsc::unbounded_channel();
    {
        let mut state = state.lock().await;
        state.process_queue.push_back(command);
        state.output_map.insert(id, tx);
    }
    let result = rx.recv().await.ok_or_eyre("Couldn't receive response")?;
    {
        let mut state = state.lock().await;
        state.output_map.remove_entry(&id);
    }
    let (success, response) = match result {
        Ok(s) => (true, s),
        Err(e) => (false, e.to_string()),
    };
    tracing::debug!("Sending back to dud: success={success}, response={response:?}");
    Ok(Json(RunCommandResponse {
        success,
        response,
        id,
    }))
}

pub async fn dud_proxy_loop(state: PackedState, exit: Receiver<()>) {
    let client = reqwest::Client::new();

    let mut waiter = { state.lock().await.waiter.clone() };
    while exit.is_empty() {
        let entry = { state.lock().await.process_queue.pop_front() };
        if let Some(entry) = entry {
            let res = client
                .post(format!("http://127.0.0.1:{STUDIO_PLUGIN_PORT}/proxy"))
                .json(&entry)
                .send()
                .await;
            if let Ok(res) = res {
                let tx = {
                    state
                        .lock()
                        .await
                        .output_map
                        .remove(&entry.id.unwrap())
                        .unwrap()
                };
                let res = res
                    .json::<RunCommandResponse>()
                    .await
                    .map(|r| r.response)
                    .map_err(Into::into);
                tx.send(res).unwrap();
            } else {
                tracing::error!("Failed to proxy: {res:?}");
            };
        } else {
            waiter.changed().await.unwrap();
        }
    }
}
