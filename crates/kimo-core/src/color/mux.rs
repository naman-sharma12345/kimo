//! Staged high-bit audio mux. Keep recoverable video until final output is ready.
use std::{path::{Path,PathBuf},process::Command,time::Duration};
struct Transaction{target:PathBuf,dir:PathBuf,video:PathBuf,wav:PathBuf,output:PathBuf,moved:bool,done:bool}
impl Transaction{
 fn new(path:&str)->Result<Self,String>{
  static NEXT:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
  let target=PathBuf::from(path);let parent=target.parent().filter(|p|!p.as_os_str().is_empty()).unwrap_or(Path::new("."));
  let mut dir=None;for _ in 0..100{let n=NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed);let q=parent.join(format!(".kimo-mux-{}-{n}",std::process::id()));match std::fs::create_dir(&q){Ok(())=>{dir=Some(q);break},Err(e)if e.kind()==std::io::ErrorKind::AlreadyExists=>continue,Err(e)=>return Err(e.to_string())}}
  let dir=dir.ok_or("cannot create unique mux directory")?;let ext=target.extension().and_then(|v|v.to_str()).unwrap_or("mkv");
  Ok(Self{video:dir.join(format!("video.{ext}")),wav:dir.join("audio.wav"),output:dir.join(format!("output.{ext}")),target,dir,moved:false,done:false})
 }
 fn stage(&mut self,pcm:&[f32])->Result<(),String>{crate::audio::write_wav(self.wav.to_str().ok_or("invalid mux path")?,pcm)?;std::fs::rename(&self.target,&self.video).map_err(|e|e.to_string())?;self.moved=true;Ok(())}
 fn restore(&mut self)->Result<(),String>{if self.moved&&!self.done{std::fs::rename(&self.video,&self.target).map_err(|e|format!("video restore failed: {e}; recoverable video at {}",self.video.display()))?;self.moved=false;}std::fs::remove_dir_all(&self.dir).map_err(|e|format!("mux temp cleanup failed: {e}; directory {}",self.dir.display()))}
 fn commit(&mut self)->Result<(),String>{std::fs::rename(&self.output,&self.target).map_err(|e|e.to_string())?;self.done=true;self.moved=false;std::fs::remove_dir_all(&self.dir).map_err(|e|format!("completed mux but temp cleanup failed: {e}; directory {}",self.dir.display()))}
}
impl Drop for Transaction{fn drop(&mut self){if !self.done{let _=self.restore();}}}
pub(crate) fn audio(path:&str,pcm:&[f32],format:&str,frames:usize)->Result<(),String>{audio_impl(path,pcm,format,frames,false)}
pub(crate) fn legacy_audio(path:&str,pcm:&[f32],frames:usize)->Result<(),String>{audio_impl(path,pcm,"legacy_aac",frames,true)}
fn audio_impl(path:&str,pcm:&[f32],format:&str,frames:usize,legacy:bool)->Result<(),String>{let mut tx=Transaction::new(path)?;tx.stage(pcm)?;let mut cmd=mux_command(&tx,format,legacy);
 match super::process::wait_command(&mut cmd,super::process::encode_deadline(frames)){Ok(())=>tx.commit(),Err(e)=>match tx.restore(){Ok(())=>Err(format!("high-bit audio mux failed: {e}; completed video-only output restored")),Err(r)=>Err(format!("high-bit audio mux failed: {e}; {r}"))}}
}
fn mux_command(tx:&Transaction,format:&str,legacy:bool)->Command{let mut cmd=Command::new("ffmpeg");cmd.args(["-v","error","-y","-i"]).arg(&tx.video).arg("-i").arg(&tx.wav).args(["-c:v","copy","-c:a",if format=="ffv1_16"{"pcm_s16le"}else{"aac"}]).args(if legacy{vec!["-b:a","192k"]}else{vec![]}).arg("-shortest").arg(&tx.output);cmd}
#[cfg(test)]mod tests{use super::*;
#[test]fn failed_mux_restores_before_cleanup(){let path=std::env::temp_dir().join(format!("kimo-mux-test-{}.mkv",std::process::id()));std::fs::write(&path,b"originalvideo").unwrap();let mut tx=Transaction::new(path.to_str().unwrap()).unwrap();tx.stage(&[0.25;64]).unwrap();std::fs::write(&tx.output,b"partialoutput").unwrap();let dir=tx.dir.clone();tx.restore().unwrap();assert_eq!(std::fs::read(&path).unwrap(),b"originalvideo");assert!(!dir.exists());std::fs::remove_file(path).unwrap();}
#[test]fn transaction_unwind_restores_original(){let path=std::env::temp_dir().join(format!("kimo-mux-unwind-{}.mkv",std::process::id()));std::fs::write(&path,b"originalvideo").unwrap();let result=std::panic::catch_unwind(||{let mut tx=Transaction::new(path.to_str().unwrap()).unwrap();tx.stage(&[0.25;64]).unwrap();std::fs::write(&tx.output,b"partialoutput").unwrap();panic!("cancelled mux");});assert!(result.is_err());assert_eq!(std::fs::read(&path).unwrap(),b"originalvideo");std::fs::remove_file(path).unwrap();}
#[test]fn stalled_command_restores_and_reaps(){let path=std::env::temp_dir().join(format!("kimo-mux-stall-{}.mkv",std::process::id()));std::fs::write(&path,b"originalvideo").unwrap();let mut tx=Transaction::new(path.to_str().unwrap()).unwrap();tx.stage(&[0.25;64]).unwrap();let start=std::time::Instant::now();assert!(super::super::process::wait_command(Command::new("sleep").arg("30"),Duration::from_millis(100)).unwrap_err().contains("deadline"));tx.restore().unwrap();assert!(start.elapsed()<Duration::from_secs(2));assert_eq!(std::fs::read(&path).unwrap(),b"originalvideo");std::fs::remove_file(path).unwrap();}
#[test]fn unique_transactions_do_not_collide(){let path=std::env::temp_dir().join("unused-kimo.mkv");let a=Transaction::new(path.to_str().unwrap()).unwrap();let b=Transaction::new(path.to_str().unwrap()).unwrap();assert_ne!(a.dir,b.dir);}
}
#[cfg(test)]mod failure_tests{use super::*;
#[test]fn restore_failure_keeps_recoverable_video_and_audio(){let path=std::env::temp_dir().join(format!("kimo-mux-restorefail-{}.mkv",std::process::id()));std::fs::write(&path,b"originalvideo").unwrap();let mut tx=Transaction::new(path.to_str().unwrap()).unwrap();tx.stage(&[0.25;64]).unwrap();std::fs::create_dir(&path).unwrap();assert!(tx.restore().unwrap_err().contains("recoverable video"));assert_eq!(std::fs::read(&tx.video).unwrap(),b"originalvideo");assert!(tx.wav.exists());std::fs::remove_dir(&path).unwrap();tx.restore().unwrap();std::fs::remove_file(path).unwrap();}
#[test]fn actual_failed_mux_restores_video(){let path=std::env::temp_dir().join(format!("kimo-mux-invalid-{}.mkv",std::process::id()));std::fs::write(&path,b"not-a-video").unwrap();let error=audio(path.to_str().unwrap(),&[0.25;64],"ffv1_16",1).unwrap_err();assert!(error.contains("video-only output restored"));assert!(error.contains("process stderr"));assert_eq!(std::fs::read(&path).unwrap(),b"not-a-video");std::fs::remove_file(path).unwrap();}
}

#[cfg(test)]mod legacy_options{use super::*;
#[test]fn legacy_aac_argv_preserves_bitrate(){let tx=Transaction::new("proof.mp4").unwrap();let cmd=mux_command(&tx,"legacy_aac",true);let args:Vec<_>=cmd.get_args().map(|v|v.to_string_lossy().to_string()).collect();assert_eq!(args,vec!["-v".to_string(),"error".into(),"-y".into(),"-i".into(),tx.video.display().to_string(),"-i".into(),tx.wav.display().to_string(),"-c:v".into(),"copy".into(),"-c:a".into(),"aac".into(),"-b:a".into(),"192k".into(),"-shortest".into(),tx.output.display().to_string()]);}
}
