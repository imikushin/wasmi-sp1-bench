;; Iterative Fibonacci. After n steps the result is fib(n) mod 2^64
;; (i64.add wraps). n is fixed at 10000 by the guest so the interpreter
;; loop dominates module startup.
(module
  (func (export "fib") (param $n i32) (result i64)
    (local $a i64)
    (local $b i64)
    (local $i i32)
    (local $sum i64)
    (local.set $a (i64.const 0))
    (local.set $b (i64.const 1))
    (local.set $i (i32.const 0))
    (block $done
      (loop $loop
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (local.set $sum (i64.add (local.get $a) (local.get $b)))
        (local.set $a (local.get $b))
        (local.set $b (local.get $sum))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $loop)))
    (local.get $a)))
