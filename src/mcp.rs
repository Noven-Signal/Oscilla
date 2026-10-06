use std::time::Duration;

use rmcp::{handler::server::wrapper::Parameters, schemars::JsonSchema, tool, tool_router};
use serde::Deserialize;
use tokio::sync::{
    mpsc::{UnboundedSender},
    oneshot,
};

use crate::{
    app::{App, MCPRequest, McpRequestType, McpResult},
    app_state::app_state::{AppStateContainer, VeSelectedTab},
};

#[derive(Deserialize, JsonSchema)]
struct EmptyParam {}

#[derive(Deserialize, JsonSchema)]
struct SetVolParam {
    vol: u16,
}

#[derive(Deserialize, JsonSchema)]
struct ChangeVeParam {
    ve: VeSelectedTab,
}

#[derive(Deserialize, JsonSchema)]
struct SeekParam {
    move_amout: u16,
}

#[derive(Deserialize, JsonSchema)]
struct AddFilesParam {
    files_path: Vec<String>,
}

pub struct McpServerHandler {
    pub mcp_request_signal_sender: UnboundedSender<MCPRequest>,
}

impl McpServerHandler {
    async fn handle_request(&self, mcp_request_type: McpRequestType)->String {
        let (call_back_sender, call_back_sender_recv) = oneshot::channel();
        let res = self.mcp_request_signal_sender.send(MCPRequest {
            request_type: mcp_request_type,
            call_back_sender,
        });
        match res {
            Ok(_) => match call_back_sender_recv.await {
                Ok(McpResult::Success) => "sucessful".to_owned(),
                Ok(McpResult::Fail(message)) => message,
                _ => "fail".to_owned(),
            },
            Err(_) => "fail".to_owned(),
        }
    }
}

#[tool_router(server_handler)]
impl McpServerHandler {
    #[tool(description = "Play")]
    async fn play(&self, _input: Parameters<EmptyParam>) -> String {
       self.handle_request(McpRequestType::StartPlayer).await
    }
    #[tool(description = "Pause")]
    async fn pause(&self, _input: Parameters<EmptyParam>) -> String {
       self.handle_request(McpRequestType::PausePlayer).await
    }
    #[tool(description = "Resume")]
    async fn resume(&self, _input: Parameters<EmptyParam>) -> String {
        self.handle_request(McpRequestType::ResumePlayer).await
    }
    #[tool(description = "Stop playing")]
    async fn stop(&self, _input: Parameters<EmptyParam>) -> String {
       self.handle_request(McpRequestType::StopPlayer).await
    }
    #[tool(description = "Seek Rewind(specify move second)")]
    async fn seek_prev(&self, input: Parameters<SeekParam>) -> String {
       self.handle_request(McpRequestType::SeekPrev(input.0.move_amout)).await
    }

    #[tool(description = "Seek Forward(specify move second)")]
    async fn seek_forward(&self, input: Parameters<SeekParam>) -> String {
      self.handle_request(McpRequestType::SeekForward(input.0.move_amout)).await
    }

    #[tool(description = "Play next track")]
    async fn play_next(&self, _input: Parameters<EmptyParam>) -> String {
        self.handle_request(McpRequestType::PlayNext).await
    }
    #[tool(description = "Play previous track")]
    async fn play_prev(&self, _input: Parameters<EmptyParam>) -> String {
        self.handle_request(McpRequestType::PlayPrev).await
    }
    #[tool(description = "set volume range of 0 to 100")]
    async fn set_vol(&self, input: Parameters<SetVolParam>) -> String {
        self.handle_request(McpRequestType::SetVol(input.0.vol)).await
    }
    
    #[tool(
        description = "enable/disable or switch visual effect.currently only oscilloscope is available"
    )]
    async fn change_ve(&self, input: Parameters<ChangeVeParam>) -> String {
        self.handle_request(McpRequestType::ChangeVe(input.0.ve)).await
    }

    #[tool(
        description = "Add audio files. Supported extensions: aif, aiff, caf, mp4, m4a, m4p, m4b, m4r, m4v, mov, mkv, webm, ogg, wav, aac, flac, mp1, mp2, mp3, mpa, opus, wv"
    )]
    async fn add_files(&self, input: Parameters<AddFilesParam>) -> String {
       self.handle_request(McpRequestType::AddFiles(input.0.files_path)).await
    }
}

impl McpServerHandler {
    pub fn handle_mcp_request(
        app_state_container: &mut AppStateContainer,
        mcp_request_type: McpRequestType,
    ) -> McpResult {
        use McpRequestType::*;
        let result = match mcp_request_type {
            StartPlayer => App::play_track(app_state_container, 0),
            PausePlayer => App::pause(app_state_container),
            ResumePlayer => App::resume(app_state_container),
            StopPlayer => App::stop_player(app_state_container, None),
            SeekPrev(move_amout) => {
                App::seek_prev(app_state_container, Duration::from_secs(move_amout as u64))
            }
            SeekForward(move_amout) => {
                App::seek_forward(app_state_container, Duration::from_secs(move_amout as u64))
            }
            PlayNext => App::play_next(app_state_container),
            PlayPrev => App::play_previous(app_state_container),
            SetVol(vol) => App::set_vol(app_state_container, vol),
            ChangeVe(target_tab) => App::change_ve_tab(app_state_container, target_tab),
            AddFiles(items) => App::add_new_files_proc(app_state_container, items),
        };
        match result {
            Ok(_) => McpResult::Success,
            Err(e) => McpResult::Fail(e),
        }
    }
}
