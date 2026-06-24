(module
  (memory (export "memory") 1024)
  (func (export "odc_alloc") (param $len i32) (result i32)
    (i32.const 0))
  (func (export "odc_run") (param $ptr i32) (param $len i32) (result i64)
    (i64.const 0)))
