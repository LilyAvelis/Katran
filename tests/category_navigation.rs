use katran::{Command, Request, Service, interaction::ShelfTab};
use serde_json::Value;
fn call(api: &mut Service, command: Command) -> Value {
    let reply=api.execute(Request{api_version:1,request_id:"navigation".into(),command});
    assert_eq!(reply["ok"],true,"{reply}"); reply["result"].clone()
}
#[test]
fn arrows_follow_visible_order_exit_pinned_and_never_enter_folder() {
    let root=tempfile::tempdir().unwrap();
    let mut api=Service::open(root.path(),10000).unwrap();
    call(&mut api,Command::SetTab{tab:ShelfTab::Images});
    call(&mut api,Command::SelectTabPosition{position:1});
    let state=call(&mut api,Command::NavigateTab{delta:-1});
    assert_eq!(state["tab"],"images"); assert_eq!(state["pinned_only"],false);
    call(&mut api,Command::SelectTabPosition{position:1});
    let state=call(&mut api,Command::RunAction{id:"tab_next".into()});
    assert_eq!(state["tab"],"screenshots"); assert_eq!(state["pinned_only"],false);
    call(&mut api,Command::SetTabEnabled{tab:ShelfTab::Screenshots,enabled:false});
    call(&mut api,Command::SetTab{tab:ShelfTab::Images});
    assert_eq!(call(&mut api,Command::NavigateTab{delta:1})["tab"],"downloads");
    call(&mut api,Command::MoveTab{tab:ShelfTab::Downloads,index:0});
    assert_eq!(call(&mut api,Command::NavigateTab{delta:1})["tab"],"images");
    assert_eq!(call(&mut api,Command::NavigateTab{delta:1})["tab"],"images");
    call(&mut api,Command::SetTab{tab:ShelfTab::All});
    assert_eq!(call(&mut api,Command::NavigateTab{delta:1})["tab"],"folder");
}
