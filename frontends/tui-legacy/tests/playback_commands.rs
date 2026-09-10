use malus::engine::{
    BrowserProcess, CdpClient, MusicKitDriver, ProfileManager, select_best_browser,
};
use serde_json::json;
use std::sync::Arc;
#[tokio::test]
#[ignore = "requires Chromium; account tests also require malus --login"]
async fn queue_reordering_is_atomic_and_play_errors_propagate() {
    let path = std::env::temp_dir().join(format!("malus_commands_{}", std::process::id()));
    let candidate = select_best_browser(None).expect("browser required");
    let browser = BrowserProcess::spawn(
        &candidate,
        ProfileManager::with_custom_path(&path),
        false,
        "about:blank",
    )
    .unwrap();
    let cdp = Arc::new(
        CdpClient::connect_to_page(browser.port, "about:blank")
            .await
            .unwrap(),
    );
    cdp.evaluate_js(r#"window.calls=[];window.MusicKit={getInstance:()=>({
        queue:window.queue,
        setQueue:async()=>{throw new Error('Subscription rejected playback');},
        play:async()=>{window.playCalled=true;}
    })};window.queue={items:[{id:'a'},{id:'b'},{id:'c'}],position:0,get length(){return this.items.length;},
        splice(index,count,items=[]){if(!Array.isArray(items))throw new Error('items must be an array');window.calls.push([index,count,items]);this.items.splice(index,count,...items);}
    };"#).await.unwrap();
    let driver = MusicKitDriver::new(cdp.clone());
    driver
        .command("move", json!([{"index":1,"id":"c"},{"index":0,"id":"b"}]))
        .await
        .unwrap();
    assert_eq!(
        cdp.evaluate_js("queue.items.map(x=>x.id)").await.unwrap(),
        json!(["a", "c", "b"])
    );
    assert_eq!(cdp.evaluate_js("calls.length").await.unwrap(), json!(1));
    assert!(
        driver
            .command("remove", json!({"index":0,"id":"b"}))
            .await
            .is_err()
    );
    assert_eq!(
        cdp.evaluate_js("queue.items.map(x=>x.id)").await.unwrap(),
        json!(["a", "c", "b"])
    );
    let error = driver.play_track_by_id("123").await.unwrap_err();
    assert!(error.to_string().contains("Subscription rejected playback"));
    assert_eq!(
        cdp.evaluate_js("typeof window.playCalled").await.unwrap(),
        json!("undefined")
    );
    assert!(driver.seek_to_time(f64::NAN).await.is_err());
    assert!(driver.set_volume(f64::INFINITY).await.is_err());
    drop(driver);
    drop(cdp);
    drop(browser);
    let _ = std::fs::remove_dir_all(path);
}
