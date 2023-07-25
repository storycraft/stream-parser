/*
 * Created on Sat Jul 22 2023
 *
 * Copyright (c) storycraft. Licensed under the MIT Licence.
 */

use std::{
    io::{self, ErrorKind, Read, Write, Seek, SeekFrom},
    pin::Pin,
    task::{Context, Poll},
};

use futures::{AsyncRead, AsyncWrite, AsyncSeek};

#[derive(Debug, Clone)]
#[pin_project::pin_project]
pub struct AsyncAdapter<S>(S);

impl<S> AsyncAdapter<S> {
    pub(crate) const fn new(stream: S) -> Self {
        Self(stream)
    }
}

impl<S: Read> AsyncRead for AsyncAdapter<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        io_result_to_poll(self.0.read(buf))
    }
}

impl<S: Write> AsyncWrite for AsyncAdapter<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        io_result_to_poll(self.0.write(buf))
    }

    fn poll_flush(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        io_result_to_poll(self.0.flush())
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl<S: Seek> AsyncSeek for AsyncAdapter<S> {
    fn poll_seek(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        pos: SeekFrom,
    ) -> Poll<io::Result<u64>> {
        io_result_to_poll(self.0.seek(pos))
    }
}

#[inline]
pub fn io_result_to_poll<T>(res: io::Result<T>) -> Poll<io::Result<T>> {
    match res {
        Err(err) if err.kind() == ErrorKind::WouldBlock => Poll::Pending,

        res => Poll::Ready(res),
    }
}
