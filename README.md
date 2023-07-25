# Stream parser
> Stop writing two version of io parser

Write asynchronous version of parse function. Use `StreamParser` to automatically supports parsing synchronously, asynchronously in both blocking, nonblocking mode. The created parser is runtime-independent and its read operation is stateful(cancel safe).

## Example
sample.txt
```
Hello world!
```

main.rs
```rust ignore
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // read constant amount of bytes
    // Must be woken by underlying `AsyncRead`, otherwise it will panic
    async fn read_fn<const SIZE: usize>(
        mut reader: Pin<&mut impl AsyncRead>,
    ) -> io::Result<[u8; SIZE]> {
        let mut arr = [0_u8; SIZE];
        reader.read_exact(&mut arr[..SIZE]).await?;

        Ok(arr)
    }

    let mut parser = StreamParser::new(read_fn::<5>);

    let mut sync_io = std::fs::File::open("./sample.txt")?;
    // read "Hello" synchronously
    dbg!(&std::str::from_utf8(&parser.read(&mut sync_io)?)?);

    let mut async_io = tokio::fs::File::open("./sample.txt")
        .await?
        .compat();

    // read "Hello" asynchronously
    dbg!(&std::str::from_utf8(
        &parser.read_async(&mut async_io).await?
    )?);

    Ok(())
}
```

## License
MIT
