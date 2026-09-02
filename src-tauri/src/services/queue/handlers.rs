use anyhow::{anyhow, Context};
use std::{
    path::{Path, PathBuf},
    pin::pin,
    sync::Arc,
};
use tauri_plugin_http::reqwest;
use tokio::fs;

use tauri_plugin_shell::{process::CommandEvent, ShellExt};

use crate::{
    aria2c, config, ffmpeg,
    shared::{get_app_handle, get_image, get_unique_path, WORKING_PATH},
    TauriError, TauriResult,
};

use super::{
    frontend::{self, RequestAction, TaskPrepareResp},
    runtime::{CtrlEvent, CtrlHandle, RUNTIME},
    scheduler::Scheduler,
    task::{MediaPaths, SubTask, Task, TaskType},
    types::MediaNfoThumb,
};

#[derive(Clone, Debug)]
pub struct SubTaskReq {
    pub task: Arc<Task>,
    pub subtask: SubTask,
    pub temp: PathBuf,
    pub folder: PathBuf,
    pub filename: String,
}

fn get_ext(task_type: &TaskType, abr: usize) -> &'static str {
    match task_type {
        TaskType::Audio => {
            if abr == 30250 {
                "eac3"
            } else if abr == 30251 || abr == 30252 {
                "flac"
            } else {
                "m4a"
            }
        }
        TaskType::AudioVideo => {
            if abr == 30251 || abr == 30252 {
                "mkv"
            } else {
                "mp4"
            }
        }
        _ => "mp4",
    }
}

fn media_urls<'a>(
    task_type: &TaskType,
    video_urls: &'a Option<Vec<String>>,
    audio_urls: &'a Option<Vec<String>>,
) -> TauriResult<&'a Vec<String>> {
    let urls = if task_type == &TaskType::Video {
        video_urls.as_ref()
    } else if task_type == &TaskType::Audio {
        audio_urls.as_ref()
    } else {
        None
    }
    .ok_or(anyhow!("No urls for type {task_type:?} found"))?;
    Ok(urls)
}

async fn handle_opus_images(req: &SubTaskReq, _ctrl: Arc<CtrlHandle>) -> TauriResult<()> {
    let subtask = &req.subtask;
    let id = &req.task.id;
    let sub_id = &subtask.id;

    subtask.send(0, 0).await?;

    let thumbs =
        frontend::request::<Vec<String>>(id, Some(sub_id), &RequestAction::GetOpusImages).await?;

    let content = thumbs.len() as u64;

    for (index, thumb) in thumbs.iter().enumerate() {
        let url = reqwest::Url::parse(thumb)?;

        let segs = url
            .path_segments()
            .ok_or(anyhow!("Failed to get path segments: {url:?}"))?;
        let segs_path = segs.collect::<PathBuf>();
        let ext = segs_path
            .extension()
            .and_then(|s| s.to_str())
            .ok_or(anyhow!("Failed to get extension from {segs_path:?}"))?;

        let path = get_unique_path(
            req.folder
                .join(format!("{}.{}.{}", &req.filename, index, ext)),
        );
        get_image(&path, thumb).await?;
        subtask.send(content, index as u64).await?;
    }

    subtask.send(content, content).await?;
    Ok(())
}

async fn handle_opus_content(req: &SubTaskReq, _ctrl: Arc<CtrlHandle>) -> TauriResult<()> {
    let subtask = &req.subtask;
    let id = &req.task.id;
    let sub_id = &subtask.id;

    subtask.send(1, 0).await?;

    let result =
        frontend::request::<Vec<u8>>(id, Some(sub_id), &RequestAction::GetOpusContent).await?;

    let output_file = get_unique_path(req.folder.join(format!("{}.md", &req.filename)));
    fs::write(&output_file, &*result).await?;

    subtask.send(1, 1).await?;
    Ok(())
}

async fn handle_subtitle(req: &SubTaskReq, _ctrl: Arc<CtrlHandle>) -> TauriResult<()> {
    let subtask = &req.subtask;
    let id = &req.task.id;
    let sub_id = &subtask.id;

    subtask.send(1, 0).await?;

    let result =
        frontend::request::<Vec<u8>>(id, Some(sub_id), &RequestAction::GetSubtitle).await?;

    let select = &req.task.select.read().await;
    let lang = select
        .misc
        .subtitles
        .as_str()
        .ok_or(anyhow!("No subtitle lang found"))?;
    let output_file = get_unique_path(req.folder.join(format!("{}.{lang}.srt", &req.filename)));
    fs::write(&output_file, &*result).await?;

    subtask.send(1, 1).await?;
    Ok(())
}

async fn handle_ai_summary(req: &SubTaskReq, _ctrl: Arc<CtrlHandle>) -> TauriResult<()> {
    let subtask = &req.subtask;
    let id = &req.task.id;
    let sub_id = &subtask.id;

    subtask.send(1, 0).await?;

    let result =
        frontend::request::<Vec<u8>>(id, Some(sub_id), &RequestAction::GetAISummary).await?;

    let output_file = get_unique_path(req.folder.join(format!("{}.md", &req.filename)));
    fs::write(&output_file, &*result).await?;

    subtask.send(1, 1).await?;
    Ok(())
}

async fn handle_nfo(req: &SubTaskReq, _ctrl: Arc<CtrlHandle>, folder: &Path) -> TauriResult<()> {
    let subtask = &req.subtask;
    let id = &req.task.id;
    let sub_id = &subtask.id;
    let nfo = &req.task.nfo.read().await;

    subtask.send(1, 0).await?;

    let data = frontend::request::<Vec<u8>>(id, Some(sub_id), &RequestAction::GetNfo).await?;

    let output_file = if subtask.task_type == TaskType::AlbumNfo {
        folder.join("tvshow.nfo")
    } else {
        req.folder.join(format!("{}.nfo", &req.filename))
    };
    fs::write(&output_file, &*data).await?;

    if subtask.task_type == TaskType::AlbumNfo {
        let path = folder.join("poster.jpg");
        let url = format!("{}@.jpg", nfo.thumbs[0].url);
        get_image(&path, &url).await?;
    }

    subtask.send(1, 1).await?;
    Ok(())
}

async fn handle_danmaku(req: &SubTaskReq, ctrl: Arc<CtrlHandle>) -> TauriResult<()> {
    let subtask = &req.subtask;
    let id = &req.task.id;
    let sub_id = &subtask.id;

    subtask.send(1, 0).await?;

    let danmaku =
        frontend::request::<Vec<u8>>(id, Some(sub_id), &RequestAction::GetDanmaku).await?;

    let xml = req.temp.join("raw.xml");
    let ass = req.temp.join("out.ass");

    fs::write(&xml, &*danmaku).await?;
    let output_file = req.folder.join(&*req.filename);
    let output_file = output_file.to_string_lossy();
    let config = config::read();

    if !config.convert.danmaku {
        fs::copy(
            &xml,
            get_unique_path(PathBuf::from(format!("{output_file}.xml"))),
        )
        .await?;
        return Ok(());
    }

    const NAME: &str = "DanmakuFactory";

    let cfg = WORKING_PATH.join("DanmakuFactory.json");
    if !cfg.exists() {
        fs::write(&cfg, &[]).await?;
    }

    let (mut child_rx, child) = get_app_handle()
        .shell()
        .sidecar(&*config.sidecar.danmakufactory)?
        .args([
            "-c",
            cfg.to_string_lossy().as_ref(),
            "-i",
            xml.to_string_lossy().as_ref(),
            "-o",
            ass.to_string_lossy().as_ref(),
            "--ignore-warnings",
        ])
        .spawn()?;

    ctrl.reg_cleaner(async move {
        child.kill()?;
        Ok(())
    })
    .await;

    let mut stderr: Vec<String> = vec![];

    while let Some(msg) = child_rx.recv().await {
        match msg {
            CommandEvent::Stdout(line) => {
                log::info!("{NAME} STDOUT: {}", String::from_utf8_lossy(&line));
            }
            CommandEvent::Stderr(line) => {
                let line = String::from_utf8_lossy(&line);
                log::warn!("{NAME} STDERR: {line}");
                stderr.push(line.into());
            }
            CommandEvent::Error(line) => {
                log::error!("{NAME} ERROR: {line}");
            }
            CommandEvent::Terminated(msg) => {
                let code = msg.code.unwrap_or(-1);
                if code != 0 {
                    return Err(TauriError::new(
                        format!("{NAME} task failed\n{}", stderr.join("\n")),
                        Some(code as isize),
                    ));
                }
            }
            _ => (),
        }
    }

    if !ass.exists() {
        // no elems
        fs::write(&ass, &[]).await?;
    }

    fs::copy(
        &ass,
        get_unique_path(PathBuf::from(format!("{output_file}.ass"))),
    )
    .await?;
    subtask.send(1, 1).await?;
    Ok(())
}

async fn handle_thumbs(req: &SubTaskReq, _ctrl: Arc<CtrlHandle>) -> TauriResult<()> {
    let subtask = &req.subtask;
    let id = &req.task.id;
    let sub_id = &subtask.id;

    subtask.send(0, 0).await?;

    let thumbs =
        frontend::request::<Vec<MediaNfoThumb>>(id, Some(sub_id), &RequestAction::GetThumbs)
            .await?;

    let content = thumbs.len() as u64;

    for (index, thumb) in thumbs.iter().enumerate() {
        let url = format!("{}@.jpg", thumb.url);
        let path = get_unique_path(
            req.folder
                .join(format!("{}.{}.jpg", &req.filename, thumb.id)),
        );
        get_image(&path, &url).await?;
        subtask.send(content, index as u64).await?;
    }

    subtask.send(content, content).await?;

    Ok(())
}

async fn post_media(
    req: &SubTaskReq,
    ctrl: Arc<CtrlHandle>,
    input: PathBuf,
) -> TauriResult<PathBuf> {
    let subtask = &req.subtask;
    let select = &req.task.select.read().await;
    let config = config::read();

    let abr = select.abr.unwrap_or(0);
    let mut ext = get_ext(&subtask.task_type, abr).to_string();
    let mut path = input;

    if subtask.task_type == TaskType::Audio {
        if config.convert.mp3 {
            ext = "mp3".into();
            path = ffmpeg::convert_mp3(req, ctrl.clone(), &path).await?;
        }
    } else if config.convert.mp4 {
        ext = "mp4".into();
        path = ffmpeg::convert_mp4(req, ctrl.clone(), &path).await?;
    }

    if config.add_metadata && ext != "eac3" {
        path = ffmpeg::add_meta(req, ctrl.clone(), &path, &ext).await?;
    }

    // Issue#198
    let output = req
        .folder
        .join(&*req.filename)
        .with_file_name(format!("{}.{}", req.filename, ext));
    if select.media.video || select.media.audio || subtask.task_type == TaskType::AudioVideo {
        let output = get_unique_path(output);
        fs::copy(&path, &output).await?;
    }

    Ok(path)
}

async fn handle_merge(
    req: &SubTaskReq,
    ctrl: Arc<CtrlHandle>,
    video_path: &Option<PathBuf>,
    audio_path: &Option<PathBuf>,
) -> TauriResult<()> {
    let subtask = &req.subtask;
    let select = &req.task.select.read().await;

    let video = video_path
        .as_ref()
        .ok_or(anyhow!("No path for video found"))?;
    let audio = audio_path
        .as_ref()
        .ok_or(anyhow!("No path for audio found"))?;

    let abr = select.abr.unwrap_or(0);
    let ext = get_ext(&subtask.task_type, abr);

    let path = ffmpeg::merge(req, ctrl.clone(), video, audio, ext).await?;
    post_media(req, ctrl, path).await?;
    Ok(())
}

async fn handle_media_download(
    req: &SubTaskReq,
    ctrl: Arc<CtrlHandle>,
    urls: &Vec<String>,
) -> TauriResult<PathBuf> {
    let gid = &req.subtask.id;

    let mut process = pin!(aria2c::download(req, ctrl.clone(), urls));
    let mut rx = ctrl.tx.subscribe();
    loop {
        tokio::select! {
            res = &mut process => break res,
            Ok(msg) = rx.recv() => match msg {
                CtrlEvent::Pause => {
                    aria2c::pause(gid).await?;
                },
                CtrlEvent::Resume => {
                    aria2c::resume(gid).await?;
                },
                _ => (),
            }
        }
    }
}

async fn build_subtask_req(
    task: &Arc<Task>,
    temp_root: &Path,
    folder: &Path,
    subtask: SubTask,
) -> TauriResult<SubTaskReq> {
    let sub_id = subtask.id.clone();
    let temp = temp_root.join(&sub_id);
    let temp_str = temp.to_string_lossy().into_owned();
    fs::create_dir_all(&*temp)
        .await
        .context(format!("Failed to create temp folder {temp_str}"))?;

    let filename =
        frontend::request::<String>(task.id.as_str(), Some(&sub_id), &RequestAction::GetFilename)
            .await?;

    subtask.reg_task(task);

    Ok(SubTaskReq {
        task: task.clone(),
        subtask,
        temp,
        folder: folder.to_path_buf(),
        filename,
    })
}

pub async fn handle_download(
    scheduler: Arc<Scheduler>,
    temp_root: &Path,
    task: Arc<Task>,
) -> TauriResult<()> {
    let id = &task.id;

    let prepare =
        frontend::request::<TaskPrepareResp>(id, None, &RequestAction::PrepareTask).await?;

    let folder = if config::read().organize.sub_folder {
        scheduler.folder.join(&*prepare.sub_folder)
    } else {
        scheduler.folder.clone()
    };

    task.prepare(&prepare, folder.clone()).await?;

    let folder_str = folder.to_string_lossy().into_owned();
    fs::create_dir_all(&folder_str)
        .await
        .context(format!("Failed to create output folder {folder_str}"))?;

    let ctrl = RUNTIME.ctrl.get_handle(id).await?;
    let temp_str = temp_root.to_string_lossy().into_owned();
    ctrl.reg_cleaner(async move {
        fs::remove_dir_all(&temp_str)
            .await
            .context(format!("Failed to remove temp folder {temp_str}"))?;
        Ok(())
    })
    .await;

    let video_subtask = prepare
        .subtasks
        .iter()
        .find(|subtask| subtask.task_type == TaskType::Video)
        .cloned();
    let audio_subtask = prepare
        .subtasks
        .iter()
        .find(|subtask| subtask.task_type == TaskType::Audio)
        .cloned();

    let video_req = match video_subtask {
        Some(subtask) => Some(build_subtask_req(&task, temp_root, &folder, subtask).await?),
        None => None,
    };
    let audio_req = match audio_subtask {
        Some(subtask) => Some(build_subtask_req(&task, temp_root, &folder, subtask).await?),
        None => None,
    };

    let mut media_paths = MediaPaths::default();

    match (video_req, audio_req) {
        (Some(video_req), Some(audio_req)) => {
            let (video_res, audio_res) = tokio::join!(
                scheduler.try_join(id, &video_req.subtask.id, async {
                    handle_media_download(
                        &video_req,
                        ctrl.clone(),
                        media_urls(
                            &video_req.subtask.task_type,
                            &prepare.video_urls,
                            &prepare.audio_urls,
                        )?,
                    )
                    .await
                }),
                scheduler.try_join(id, &audio_req.subtask.id, async {
                    handle_media_download(
                        &audio_req,
                        ctrl.clone(),
                        media_urls(
                            &audio_req.subtask.task_type,
                            &prepare.video_urls,
                            &prepare.audio_urls,
                        )?,
                    )
                    .await
                }),
            );
            media_paths.video = Some(video_res?);
            media_paths.audio = Some(audio_res?);
        }
        (Some(req), None) | (None, Some(req)) => {
            let path = scheduler
                .try_join(id, &req.subtask.id, async {
                    handle_media_download(
                        &req,
                        ctrl.clone(),
                        media_urls(
                            &req.subtask.task_type,
                            &prepare.video_urls,
                            &prepare.audio_urls,
                        )?,
                    )
                    .await
                })
                .await?;
            if req.subtask.task_type == TaskType::Video {
                media_paths.video = Some(path);
            } else {
                media_paths.audio = Some(path);
            }
        }
        (None, None) => {}
    }

    *task.media_paths.write().await = media_paths;
    Ok(())
}

pub async fn handle_postprocess(
    scheduler: Arc<Scheduler>,
    temp_root: &Path,
    task: Arc<Task>,
) -> TauriResult<()> {
    let id = &task.id;
    let folder = task.folder.read().await.clone();
    let subtasks = task.subtasks.read().await.clone();

    let ctrl = RUNTIME.ctrl.get_handle(id).await?;

    let video_subtask = subtasks
        .iter()
        .find(|subtask| subtask.task_type == TaskType::Video)
        .cloned();
    let audio_subtask = subtasks
        .iter()
        .find(|subtask| subtask.task_type == TaskType::Audio)
        .cloned();

    let video_req = match video_subtask {
        Some(subtask) => Some(build_subtask_req(&task, temp_root, &folder, subtask).await?),
        None => None,
    };
    let audio_req = match audio_subtask {
        Some(subtask) => Some(build_subtask_req(&task, temp_root, &folder, subtask).await?),
        None => None,
    };

    let mut video_path = task.media_paths.read().await.video.clone();
    let mut audio_path = task.media_paths.read().await.audio.clone();

    if let (Some(req), Some(path)) = (video_req.as_ref(), video_path.as_ref()) {
        video_path = Some(post_media(req, ctrl.clone(), path.clone()).await?);
    }
    if let (Some(req), Some(path)) = (audio_req.as_ref(), audio_path.as_ref()) {
        audio_path = Some(post_media(req, ctrl.clone(), path.clone()).await?);
    }

    for subtask in subtasks.iter().cloned() {
        if subtask.task_type == TaskType::Video || subtask.task_type == TaskType::Audio {
            continue;
        }

        let sub_id = subtask.id.clone();
        let task_type = subtask.task_type.clone();

        log::info!("Handling Subtask#{sub_id}\n    type: {task_type:?}\n    Task#{id}",);

        let temp = temp_root.join(&sub_id);
        let temp_str = temp.to_string_lossy().into_owned();
        fs::create_dir_all(&*temp)
            .await
            .context(format!("Failed to create temp folder {temp_str}"))?;

        let filename =
            frontend::request::<String>(id, Some(&sub_id), &RequestAction::GetFilename).await?;

        subtask.reg_task(&task);

        let ctrl = ctrl.clone();

        let req = SubTaskReq {
            task: task.clone(),
            subtask,
            temp,
            folder: folder.clone(),
            filename,
        };
        scheduler
            .try_join(id, &sub_id, async {
                match task_type {
                    TaskType::Video | TaskType::Audio => {
                        unreachable!("media subtasks are handled before the main loop")
                    }
                    TaskType::AudioVideo => {
                        handle_merge(&req, ctrl, &video_path, &audio_path).await
                    }
                    TaskType::Thumb => handle_thumbs(&req, ctrl).await,
                    TaskType::LiveDanmaku | TaskType::HistoryDanmaku => {
                        handle_danmaku(&req, ctrl).await
                    }
                    TaskType::AlbumNfo | TaskType::SingleNfo => {
                        handle_nfo(&req, ctrl, &folder).await
                    }
                    TaskType::AiSummary => handle_ai_summary(&req, ctrl).await,
                    TaskType::Subtitles => handle_subtitle(&req, ctrl).await,
                    TaskType::OpusContent => handle_opus_content(&req, ctrl).await,
                    TaskType::OpusImages => handle_opus_images(&req, ctrl).await,
                }
            })
            .await?;
    }

    Ok(())
}
