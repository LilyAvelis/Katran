use katran::{Command,Request,Service,clipboard_files};
use serde_json::Value;
use std::{path::PathBuf,sync::{Arc,atomic::{AtomicU32,Ordering}}};
struct Fake { sequence:Arc<AtomicU32>,paths:Vec<PathBuf> }
impl clipboard_files::Source for Fake {
    fn sequence(&self)->u32 {self.sequence.load(Ordering::SeqCst)}
    fn files(&self)->katran::Result<Vec<PathBuf>> {Ok(self.paths.clone())}
}
fn call(api:&mut Service,command:Command)->Value {
    let r=api.execute(Request{api_version:1,request_id:"paste".into(),command});assert_eq!(r["ok"],true,"{r}");r["result"].clone()
}
#[test]
fn external_clipboard_replaces_staged_cut_and_never_ingests_or_removes_sources() {
    let root=tempfile::tempdir().unwrap();let source=tempfile::tempdir().unwrap();let target=tempfile::tempdir().unwrap();
    let path=source.path().join("exact.csv");std::fs::write(&path,"α,,\r\n").unwrap();
    let mut api=Service::open(root.path(),100000).unwrap();
    let sequence=Arc::new(AtomicU32::new(10));
    api.set_clipboard_files_source(Box::new(Fake{sequence:sequence.clone(),paths:vec![path.clone()]}));
    let event=call(&mut api,Command::IngestBytes{name:"keep.png".into(),bytes:vec![1,2],source:"synthetic".into()});
    call(&mut api,Command::StageTransfer{items:vec![katran::SelectionRef{id:event["id"].as_i64().unwrap(),revision:event["revision"].as_str().unwrap().into()}],cut:true});
    call(&mut api,Command::ChooseFolder{path:target.path().into()});
    sequence.store(11,Ordering::SeqCst);
    let receipt=call(&mut api,Command::PasteToFolder);
    assert_eq!(receipt["external"],true);assert_eq!(receipt["items"][0]["status"],"copied");
    assert_eq!(std::fs::read(target.path().join("exact.csv")).unwrap(),std::fs::read(path).unwrap());
    assert_eq!(call(&mut api,Command::List{limit:100})["events"].as_array().unwrap().len(),1);
    assert_eq!(call(&mut api,Command::Get{id:event["id"].as_i64().unwrap()})["id"],event["id"]);
}
