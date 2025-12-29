This library provides the [`GrowLock<T>`] type: a contiguous array type that
can have simultaneously any number of readers **and** one writer.

This is possible because after an element is pushed onto the [`GrowLock`],
it can no longer be modified nor removed. The only way the writer can modify
the [`GrowLock`] is by pushing an element at the end of it.

If you want to modify elements of the array or have a dynamical capacity,
you should use [`Vec<T>`] instead.
