use std::collections::VecDeque;
use std::net::TcpListener;
use std::os::unix::io::{AsRawFd, RawFd};
use std::{io, ptr};

use io_uring::{opcode, squeue, types, IoUring, SubmissionQueue};
use slab::Slab;

use crate::connection::Connection;

#[derive(Clone, Debug)]
enum Token {
    Accept,
    Poll(Connection),
    Read(Connection),
    Write(Connection),
}

pub struct AcceptCount {
    entry: squeue::Entry,
    count: usize,
}

impl AcceptCount {
    fn new(fd: RawFd, token: usize, count: usize) -> AcceptCount {
        AcceptCount {
            entry: opcode::Accept::new(types::Fd(fd), ptr::null_mut(), ptr::null_mut())
                .build()
                .user_data(token as _),
            count,
        }
    }

    pub fn push_to(&mut self, sq: &mut SubmissionQueue<'_>) {
        while self.count > 0 {
            unsafe {
                match sq.push(&self.entry) {
                    Ok(_) => self.count -= 1,
                    Err(_) => break,
                }
            }
        }

        sq.sync();
    }
}

pub fn start_server_uring() -> anyhow::Result<()> {
    let mut ring = IoUring::new(256)?;
    let listener = TcpListener::bind(("127.0.0.1", 50051))?;

    let mut backlog = VecDeque::new();
    let mut token_alloc = Slab::with_capacity(64);

    println!("listen {}", listener.local_addr()?);

    let (submitter, mut sq, mut cq) = ring.split();

    let mut accept = AcceptCount::new(listener.as_raw_fd(), token_alloc.insert(Token::Accept), 3);

    accept.push_to(&mut sq);

    loop {
        match submitter.submit_and_wait(1) {
            Ok(_) => (),
            Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => (),
            Err(err) => return Err(err.into()),
        }
        cq.sync();

        // clean backlog
        loop {
            if sq.is_full() {
                match submitter.submit() {
                    Ok(_) => (),
                    Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => break,
                    Err(err) => return Err(err.into()),
                }
            }
            sq.sync();

            match backlog.pop_front() {
                Some(sqe) => unsafe {
                    let _ = sq.push(&sqe);
                },
                None => break,
            }
        }

        accept.push_to(&mut sq);

        for cqe in &mut cq {
            let ret = cqe.result();
            let token_index = cqe.user_data() as usize;

            if ret < 0 {
                eprintln!(
                    "token {:?} error: {:?}",
                    token_alloc.get(token_index),
                    io::Error::from_raw_os_error(-ret)
                );
                continue;
            }

            let token = &mut token_alloc[token_index];
            match token.clone() {
                Token::Accept => {
                    println!("accept");

                    accept.count += 1;

                    let fd = ret;
                    let conn = connection::Connection::new(srv_conn, fd);

                    let poll_token = token_alloc.insert(Token::Poll(conn));

                    let poll_e = opcode::PollAdd::new(types::Fd(fd), libc::POLLIN as _)
                        .build()
                        .user_data(poll_token as _);

                    unsafe {
                        if sq.push(&poll_e).is_err() {
                            backlog.push_back(poll_e);
                        }
                    }
                }
                Token::Poll(conn) => {
                    *token = Token::Read(conn);

                    let fd = conn.fd;
                    let buf = conn.read_input();
                    let read_e = opcode::Recv::new(types::Fd(fd), buf.as_mut_ptr(), buf.len() as _)
                        .build()
                        .user_data(token_index as _);

                    unsafe {
                        if sq.push(&read_e).is_err() {
                            backlog.push_back(read_e);
                        }
                    }
                }
                Token::Read(conn) => {
                    if ret == 0 {
                        token_alloc.remove(token_index);

                        println!("shutdown");

                        unsafe {
                            libc::close(fd);
                        }
                    } else {
                        let len = ret as usize;
                        conn.handle(len);

                        let fd = conn.fd;
                        let buf = conn.send_output();

                        *token = Token::Write(conn);

                        let write_e = opcode::Send::new(types::Fd(fd), buf.as_ptr(), len as _)
                            .build()
                            .user_data(token_index as _);

                        unsafe {
                            if sq.push(&write_e).is_err() {
                                backlog.push_back(write_e);
                            }
                        }
                    }
                }
                Token::Write(conn) => {
                    let write_len = ret as usize;
                    let buf = conn.send_output();

                    let entry = if write_len >= buf.len() {
                        let fd = conn.fd;
                        *token = Token::Poll(conn);

                        opcode::PollAdd::new(types::Fd(fd), libc::POLLIN as _)
                            .build()
                            .user_data(token_index as _)
                    } else {
                        conn.send_output_go(len);
                        let fd = conn.fd;
                        let buf = conn.send_output();
                        *token = Token::Write(conn);
                        opcode::Write::new(types::Fd(fd), buf.as_ptr(), buf.len() as _)
                            .build()
                            .user_data(token_index as _)
                    };

                    unsafe {
                        if sq.push(&entry).is_err() {
                            backlog.push_back(entry);
                        }
                    }
                }
            }
        }
    }
}
