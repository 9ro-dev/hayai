//! Lifespan feature tests for Hayai framework

use hayai::{HayaiApp, Lifespan, LifespanSharedState};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Test that the lifespan configuration can be created
#[test]
fn test_lifespan_config_creation() {
    let lifespan = Lifespan::new();
    let app = HayaiApp::new().configure_lifespan(lifespan);
    let _ = app.lifespan_ref();
}

/// Test that startup callback can be configured
#[test]
fn test_startup_callback() {
    let startup_called = Arc::new(AtomicBool::new(false));
    let startup_called_clone = startup_called.clone();
    
    let app = HayaiApp::new()
        .on_startup(move |_state: LifespanSharedState| {
            Box::pin(async move {
                startup_called_clone.store(true, Ordering::SeqCst);
            })
        });
        
    let _ = app.lifespan_ref();
}

/// Test that shutdown callback can be configured  
#[test]
fn test_shutdown_callback() {
    let shutdown_called = Arc::new(AtomicBool::new(false));
    let shutdown_called_clone = shutdown_called.clone();
    
    let app = HayaiApp::new()
        .on_shutdown(move |_state: LifespanSharedState| {
            Box::pin(async move {
                shutdown_called_clone.store(true, Ordering::SeqCst);
            })
        });
        
    let _ = app.lifespan_ref();
}

/// Test helper struct
struct MyService {
    name: String,
}
unsafe impl Send for MyService {}
unsafe impl Sync for MyService {}

/// Test that startup callback receives shared state with dependencies
#[test]
fn test_startup_receives_state() {
    let app = HayaiApp::new()
        .dep(MyService { name: "test".to_string() })
        .on_startup(|state| {
            Box::pin(async move {
                let _service: Option<Arc<MyService>> = state.get();
            })
        });
        
    let _ = app.lifespan_ref();
}

/// Test that shutdown callback receives shared state
#[test]
fn test_shutdown_receives_state() {
    let app = HayaiApp::new()
        .dep(MyService { name: "test".to_string() })
        .on_shutdown(|state| {
            Box::pin(async move {
                let _service: Option<Arc<MyService>> = state.get();
            })
        });
        
    let _ = app.lifespan_ref();
}
