RAII structure used to release the exclusive write access of a lock when
dropped.

This structure is created by the [`write`](GrowLock::write) and
[`try_write`](GrowLock::try_write) method on [`GrowLock`]
