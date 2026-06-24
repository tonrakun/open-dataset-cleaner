use crate::config::PluginConfig;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::time::Duration;
use wasmtime::{Engine, Instance, Module, Store, StoreLimits, StoreLimitsBuilder, TypedFunc};

/// プラグインがJSONで返す採用判定/スコア。フィールドはすべて任意で、
/// 値を返さない場合はそのプラグインに意見がないものとして扱う。
#[derive(Debug, Default, Deserialize)]
pub struct PluginCallResult {
    pub score: Option<f64>,
    pub accept: Option<bool>,
    pub reason: Option<String>,
}

struct PluginCtx {
    limits: StoreLimits,
}

/// ロード済みのWASMプラグイン1件。呼び出しごとに新しい`Store`/`Instance`を
/// 生成するため、共有状態を持たずスレッド間で安全に`&self`から呼び出せる。
pub struct WasmPlugin {
    name: String,
    engine: Engine,
    module: Module,
    config_value: Value,
    deadline_ticks: u64,
    memory_limit_bytes: usize,
}

impl WasmPlugin {
    pub fn load(engine: &Engine, config: &PluginConfig, epoch_tick: Duration) -> anyhow::Result<Self> {
        let module = Module::from_file(engine, &config.path).map_err(|e| {
            anyhow::anyhow!("プラグイン {} ({}) の読み込みに失敗しました: {}", config.name, config.path, e)
        })?;
        let tick_ms = epoch_tick.as_millis().max(1) as f64;
        let deadline_ticks = ((config.timeout_ms as f64) / tick_ms).ceil().max(1.0) as u64;
        Ok(Self {
            name: config.name.clone(),
            engine: engine.clone(),
            module,
            config_value: config.config_as_json(),
            deadline_ticks,
            memory_limit_bytes: config.memory_limit_bytes,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// `text`/`meta`をJSONエンコードしてプラグインの`odc_run`に渡し、結果を読み戻す。
    ///
    /// ABI: プラグインは `memory` を export し、`odc_alloc(len: i32) -> i32`
    /// (確保した先頭ポインタを返す) と `odc_run(ptr: i32, len: i32) -> i64`
    /// (出力ポインタ<<32 | 出力長 をpackして返す) を export しなければならない。
    pub fn call(&self, text: &str, meta: &Map<String, Value>) -> anyhow::Result<PluginCallResult> {
        let input = json!({ "text": text, "meta": meta, "config": self.config_value }).to_string();

        let limits = StoreLimitsBuilder::new().memory_size(self.memory_limit_bytes).build();
        let mut store = Store::new(&self.engine, PluginCtx { limits });
        store.limiter(|ctx| &mut ctx.limits);
        store.epoch_deadline_trap();
        store.set_epoch_deadline(self.deadline_ticks);

        let instance = Instance::new(&mut store, &self.module, &[])
            .map_err(|e| anyhow::anyhow!("プラグイン {} のインスタンス化に失敗しました: {}", self.name, e))?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow::anyhow!("プラグイン {} はメモリをエクスポートしていません", self.name))?;
        let alloc: TypedFunc<i32, i32> = instance
            .get_typed_func(&mut store, "odc_alloc")
            .map_err(|_| anyhow::anyhow!("プラグイン {} は odc_alloc をエクスポートしていません", self.name))?;
        let run: TypedFunc<(i32, i32), i64> = instance
            .get_typed_func(&mut store, "odc_run")
            .map_err(|_| anyhow::anyhow!("プラグイン {} は odc_run をエクスポートしていません", self.name))?;

        let input_bytes = input.as_bytes();
        let in_ptr = alloc
            .call(&mut store, input_bytes.len() as i32)
            .map_err(|e| anyhow::anyhow!("プラグイン {} の odc_alloc 呼び出しに失敗しました: {}", self.name, e))?;
        memory
            .write(&mut store, in_ptr as usize, input_bytes)
            .map_err(|e| anyhow::anyhow!("プラグイン {} への入力書き込みに失敗しました: {}", self.name, e))?;

        let packed = run
            .call(&mut store, (in_ptr, input_bytes.len() as i32))
            .map_err(|e| anyhow::anyhow!("プラグイン {} の odc_run 呼び出しに失敗しました: {}", self.name, e))?;
        let packed = packed as u64;
        let out_ptr = (packed >> 32) as usize;
        let out_len = (packed & 0xFFFF_FFFF) as usize;

        let mut out_bytes = vec![0u8; out_len];
        memory
            .read(&store, out_ptr, &mut out_bytes)
            .map_err(|e| anyhow::anyhow!("プラグイン {} の出力読み込みに失敗しました: {}", self.name, e))?;
        let out_str = String::from_utf8(out_bytes)
            .map_err(|e| anyhow::anyhow!("プラグイン {} の出力がUTF-8ではありません: {}", self.name, e))?;
        serde_json::from_str(&out_str)
            .map_err(|e| anyhow::anyhow!("プラグイン {} の出力JSONを解析できません: {}", self.name, e))
    }
}
