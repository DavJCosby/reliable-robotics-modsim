From `/app`:

```sh
cargo run --release -- --event "2s:0=open" --event "15s:0=closed"
```


Event notation : `<timestamp>s:<pipe_id>=<open/close>`
- `timestamp`: how many seconds into the simulation to trigger the event
- `pipe_id`: which pipe is associated with the event
- `open/close`: whether that pipe should become open or closed at that timestamp
