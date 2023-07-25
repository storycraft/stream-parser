/*
 * Created on Sun Jul 23 2023
 *
 * Copyright (c) storycraft. Licensed under the MIT Licence.
 */

use std::{
    error::Error,
    io::{self},
    pin::Pin,
};

use futures::{AsyncRead, AsyncReadExt};
use stream_parser::StreamParser;
use tokio_util::compat::TokioAsyncReadCompatExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    async fn read_fn<const SIZE: usize>(
        mut reader: Pin<&mut impl AsyncRead>,
    ) -> io::Result<[u8; SIZE]> {
        let mut arr = [0_u8; SIZE];
        reader.read_exact(&mut arr[..SIZE / 2]).await?;
        reader.read_exact(&mut arr[SIZE / 2..]).await?;

        Ok(arr)
    }

    let mut parser = StreamParser::new(read_fn::<5>);

    let mut sync_io = std::fs::File::open("./examples/sample.txt")?;
    dbg!(&std::str::from_utf8(&parser.read(&mut sync_io)?)?);

    let mut async_io = tokio::fs::File::open("./examples/sample.txt")
        .await?
        .compat();
    dbg!(&std::str::from_utf8(
        &parser.read_async(&mut async_io).await?
    )?);

    Ok(())
}
