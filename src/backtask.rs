use std::io::{ self, Read };
use std::sync::{ mpsc, Arc, Mutex };
use std::thread;
use std::time::{ Instant, Duration };
use std::collections::VecDeque;
use std::fs::File;

use std::os::unix::io::{ AsRawFd };
use libc::{F_GETFL, F_SETFL, fcntl, O_NONBLOCK};

use termion::event::Key;
use termion::input::TermRead;

#[cfg(feature="rustfft")]
use  {
    rustfft::algorithm::Radix4
,   rustfft::FFT
,   rustfft::num_complex::Complex
};

#[cfg(not(feature="rustfft"))]
use {
    chfft::CFft1D
,   num_complex::Complex
};

pub enum Event
{
    Tick
,   Exit
,   Sample
}

pub struct BarData
{
  pub bar_l : Vec::< f32 >
, pub bar_r : Vec::< f32 >
, pub bar_h : Vec::< u32 >
, pub sbuf  : usize
, pub fcnt  : u64
, pub rcnt  : u64
, pub scnt  : u64
, pub delay : Duration
}

pub struct BackTask
{
            rx          : mpsc::Receiver< Event >
,   pub bar_data        : Arc< Mutex < BarData > >

,   _input_handle   : thread::JoinHandle<()>
,   _tick_handle    : thread::JoinHandle<()>
,   _sampler_handle     : thread::JoinHandle<()>
}

impl BackTask
{
    pub fn new( tick_rate: u64 ) -> BackTask
    {
        let ( tx, rx ) = mpsc::channel();
        let bar_data =
            Arc::new(
                    Mutex::new(
                        BarData
                        {
                        bar_l : Vec::< f32 >::new()
                    ,   bar_r : Vec::< f32 >::new()
                    ,   bar_h : Vec::< u32 >::new()
                    ,   sbuf  : 0
                    ,   fcnt  : 0
                    ,   rcnt  : 0
                    ,   scnt  : 0
                    ,   delay : Duration::from_millis( 500 )
                        }
                    )
                );

        let input_handle =
        {
            let tx = tx.clone();
            let bar_data = bar_data.clone();

            thread::spawn( move ||
                {
                    let d10 = Duration::from_millis(10);

                    for evt in io::stdin().keys()
                    {
                        match evt
                        {
                            Ok( Key::Ctrl('c') ) =>
                            {
                                tx.send( Event::Exit ).unwrap();
                            }
                        ,   Ok( Key::Left ) | Ok( Key::Up ) | Ok( Key::PageUp ) =>
                            {
                                let mut bar_data = bar_data.lock().unwrap();
                                bar_data.delay += d10;
                            }
                        ,   Ok( Key::Right ) | Ok( Key::Down ) | Ok( Key::PageDown ) =>
                            {
                                let mut bar_data = bar_data.lock().unwrap();

                                if bar_data.delay >= d10
                                {
                                    bar_data.delay -= d10;
                                }
                            }
                        ,   Ok(_) => {}
                        ,   Err(_) => {}
                        };
                    }
                }
            )
        };

        let tick_handle =
        {
            let tx = tx.clone();

            thread::spawn( move ||
                loop
                {
                    tx.send( Event::Tick ).unwrap();
                        thread::sleep( Duration::from_millis( tick_rate ) )
                    }
                )
        };

        let sampler_handle =
        {
            let tx = tx.clone();
            let bar_data = bar_data.clone();

            thread::spawn( move ||
                    {
                    let _ = sampler( tx, bar_data ).unwrap();
                }
            )
        };

        BackTask
        {
            rx
        ,   bar_data            : bar_data.clone()
        ,   _input_handle   : input_handle
        ,   _tick_handle        : tick_handle
        ,   _sampler_handle : sampler_handle
        }
    }

    pub fn next( &self ) -> Result< Event, mpsc::RecvError >
    {
        self.rx.recv()
    }
}

fn open_fifo() -> io::Result< File >
{
    let fifo = File::open( "/tmp/mpd.fifo" )?;

    let fd = fifo.as_raw_fd();

    let flags = unsafe { fcntl( fd, F_GETFL, 0 ) };

    if flags < 0
    {
        return Err( io::Error::last_os_error() );
    }

    let flags = flags | O_NONBLOCK;

    let res = unsafe { fcntl( fd, F_SETFL, flags ) };

    if res != 0
    {
        return Err( io::Error::last_os_error() );
    }

    Ok( fifo )
}

const SAMPLING_RATE     : usize = 44100;
const CHANNELS          : usize = 2;
const F_BUF_SIZE            : usize = SAMPLING_RATE / 20;
const F_BUF_SAMPLE_SZ   : usize = 2;
const S_BUF_SIZE        : usize = 8192;
const FIFO_STALL_SLEEP  : Duration = Duration::from_millis( 10 );
const FIFO_STALL_RESET  : Duration = Duration::from_millis( 50 );
const FIFO_STALL_REOPEN : Duration = Duration::from_millis( 1000 );
const FFT_BUF_SIZE      : usize = S_BUF_SIZE / 2;
const FFT_BUF_SLIDE_SIZE    : usize = FFT_BUF_SIZE / 2;
const FFT_SPEC_SIZE         : usize = FFT_BUF_SIZE / 2;
const FFT_SPEC_HZ_D     : f32 = SAMPLING_RATE as f32 / 2.0 / FFT_SPEC_SIZE as f32;
const OCT_SCALE         : f32 = 2.0;
const ENABLE_CORRECTION : bool  = true;

fn sampler( tx : mpsc::Sender< Event >, bar_data : Arc< Mutex < BarData > > )
    -> io::Result<()>
{

    #[cfg(feature="rustfft")]
    let fft_engine_rustfft = Radix4::new( FFT_BUF_SIZE, false );

    #[cfg(not(feature="rustfft"))]
    let mut fft_engine_chfft = CFft1D::<f32>::with_len( FFT_BUF_SIZE );

    let mut f_buf = [ 0u8 ; F_BUF_SAMPLE_SZ * F_BUF_SIZE ];
    let     mut s_buf = VecDeque::< i16 >::with_capacity( S_BUF_SIZE );

    let mut fft_i_l : Vec::< Complex< f32 > > = vec![ Complex::new( 0.0, 0.0 ); FFT_BUF_SIZE ];
    let mut fft_i_r : Vec::< Complex< f32 > > = vec![ Complex::new( 0.0, 0.0 ); FFT_BUF_SIZE ];

    let mut fft_o_l : Vec::< Complex< f32 > > = vec![ Complex::new( 0.0, 0.0 ); FFT_BUF_SIZE ];
    let mut fft_o_r : Vec::< Complex< f32 > > = vec![ Complex::new( 0.0, 0.0 ); FFT_BUF_SIZE ];

    let mut fft_amp_l : Vec::< f32 > = vec![ 0.0 ; FFT_SPEC_SIZE ];
    let mut fft_amp_r : Vec::< f32 > = vec![ 0.0 ; FFT_SPEC_SIZE ];
    let mut fft_amp_b : Vec::< usize > = vec![ 0 ; FFT_SPEC_SIZE ];

    let bar_len     : usize = ( ( SAMPLING_RATE as f32 ).log2().floor() * OCT_SCALE ) as usize;

    let mut bar_amp_l : Vec::< f32 > = vec![ 0.0 ; bar_len ];
    let mut bar_amp_r : Vec::< f32 > = vec![ 0.0 ; bar_len ];
    let mut bar_amp_h : Vec::< u32 > = vec![ 0   ; bar_len ];
    let mut bar_amp_c : Vec::< f32 > = vec![ 0.0 ; bar_len ];
    let mut bar_amp_p : Vec::< f32 > = vec![ 0.0 ; bar_len ];

    let mut bar_st = 0;
    let mut bar_ed = 0;

    let mut s_buf_delay_size = 0;

    for i in 0..bar_len
    {
        let hz = 2_f32.powf( i as f32 / OCT_SCALE ) as u32;

        bar_amp_h[ i ] = hz;

        if bar_st == 0 && hz > 16
        {
            bar_st = i;
        }

        if bar_ed == 0 && hz >= 20000
        {
            bar_ed = i;
        }

        if ENABLE_CORRECTION
        {
            bar_amp_p[ i ] = i as f32 / OCT_SCALE / 4.0;
        }
        else
        {
            bar_amp_p[ i ] = 2.0;
        }
    }

    for i in 0..FFT_SPEC_SIZE
    {
        let hz = FFT_SPEC_HZ_D * ( i as f32 + 0.5 );
        let p = ( hz.log2() * OCT_SCALE ).floor() as usize;

        bar_amp_c[ p ] += 1.0;
        fft_amp_b[ i ] = p;
    }

    {
        let mut bd = bar_data.lock().unwrap();

        bd.bar_h.clear();
        bd.bar_h.extend_from_slice( &bar_amp_h[ bar_st..bar_ed ] );

        s_buf_delay_size = ( bd.delay.as_secs_f32() * ( SAMPLING_RATE * CHANNELS ) as f32 ) as usize;
    }

    let mut fifo = open_fifo()?;

    let mut fifo_stall_time : Option< Instant > = None;
    let mut fifo_stall_reset = false;

    // pre read

    loop
    {
        match fifo.read( &mut f_buf )
        {
            Err( ref x )
                if  x.kind() == io::ErrorKind::WouldBlock
            /*  ||  x.kind() == io::ErrorKind::Interrupted  */
                =>
            {
                break;
            }
        ,   Err( x ) => { return Err(x); }
        ,   Ok( _ ) => {}
        }
    }

    macro_rules! fifo_reset {
        () => {
            if let Some( x ) = fifo_stall_time
            {
                let mut bd = bar_data.lock().unwrap();
                bd.scnt += 1;

                if !fifo_stall_reset && x.elapsed() > FIFO_STALL_RESET
                {
                    for p in 0..bar_len
                    {
                        bar_amp_l[ p ] = 0.0;
                        bar_amp_r[ p ] = 0.0;
                    }

                    bd.bar_l.clear();
                    bd.bar_r.clear();

                    bd.bar_l.extend_from_slice( &bar_amp_l[ bar_st..bar_ed ] );
                    bd.bar_r.extend_from_slice( &bar_amp_r[ bar_st..bar_ed ] );
                    bd.sbuf = s_buf.len();

                    tx.send( Event::Sample ).unwrap();

                    fifo_stall_reset = true;
                }
                else if x.elapsed() > FIFO_STALL_REOPEN
                {
                    bd.rcnt += 1;

                    s_buf.clear();

                    fifo = open_fifo()?;

                    fifo_stall_time = None;
                    fifo_stall_reset = false;
                }
            }
            else
            {
                fifo_stall_time = Some( Instant::now() );
            }

            for _ in 0..( FIFO_STALL_SLEEP.as_secs_f32() * ( SAMPLING_RATE * CHANNELS ) as f32 ) as usize
            {
                s_buf.pop_front();
                s_buf.push_back( 0 );
            }

            thread::sleep( FIFO_STALL_SLEEP );
        }
    }

    loop
    {
        match fifo.read( &mut f_buf )
        {
            Err( ref x )
                if  x.kind() == io::ErrorKind::WouldBlock
            /*  ||  x.kind() == io::ErrorKind::Interrupted  */
                =>
            {
                fifo_reset!();
            }
        ,   Err( x ) => { return Err(x); }
        ,   Ok( n ) =>
            {
                if n == 0
                {
                    fifo_reset!();
                }
                else
                {
                    if let Some( _ ) = fifo_stall_time
                    {
                        fifo_stall_time = None;
                        fifo_stall_reset = false;
                    }

                    for i in 0..n / 2
                    {
                        let mut b = [ 0u8 ; 2 ];

                        b[ 0 ] = f_buf[ i * CHANNELS ];
                        b[ 1 ] = f_buf[ i * CHANNELS + 1 ];

                        let x = i16::from_le_bytes( b );
                        s_buf.push_back( x );
                    }
                }

                if s_buf.len() > FFT_BUF_SIZE * CHANNELS + s_buf_delay_size
                {
                    while s_buf.len() > FFT_BUF_SIZE * CHANNELS + s_buf_delay_size
                    {
                        s_buf.pop_front();
                    }

                    {
                        let mut s_buf_iter = s_buf.iter();

                        for i in 0..FFT_BUF_SIZE
                        {
                            let l = *s_buf_iter.next().unwrap() as f32 / std::i16::MAX as f32;
                            let r = *s_buf_iter.next().unwrap() as f32 / std::i16::MAX as f32;

                            fft_i_l[ i ] = Complex::< f32 >::new( l, 0.0 );
                            fft_i_r[ i ] = Complex::< f32 >::new( r, 0.0 );
                        }
                    }

                    for _ in 0.. ( FFT_BUF_SIZE - FFT_BUF_SLIDE_SIZE ) * CHANNELS
                    {
                        s_buf.pop_front();
                    }

                    #[cfg(feature="rustfft")]
                    {
                        fft_engine_rustfft.process( &mut fft_i_l, &mut fft_o_l );
                        fft_engine_rustfft.process( &mut fft_i_r, &mut fft_o_r );
                    }

                    #[cfg(not(feature="rustfft"))]
                    {
                        let tmp_fft_o_l = fft_engine_chfft.forward( fft_i_l.as_slice() );
                        let tmp_fft_o_r = fft_engine_chfft.forward( fft_i_r.as_slice() );

                        fft_o_l.clear();
                        fft_o_r.clear();

                        fft_o_l.extend_from_slice( &tmp_fft_o_l );
                        fft_o_r.extend_from_slice( &tmp_fft_o_r );
                    }

                    for p in 0..bar_len
                    {
                        bar_amp_l[ p ] = 0.0;
                        bar_amp_r[ p ] = 0.0;
                    }

                    for i in 0..FFT_SPEC_SIZE
                    {
                        fft_amp_l[ i ] = fft_o_l[ i ].norm_sqr().sqrt().log10() * 20.0;
                        fft_amp_r[ i ] = fft_o_r[ i ].norm_sqr().sqrt().log10() * 20.0;

                        bar_amp_l[ fft_amp_b[ i ] ] += fft_amp_l[ i ];
                        bar_amp_r[ fft_amp_b[ i ] ] += fft_amp_r[ i ];
                    }

                    for p in 0..bar_len
                    {
                        if bar_amp_c[ p ] != 0.0
                        {
                            bar_amp_l[ p ] /= bar_amp_c[ p ];
                            bar_amp_r[ p ] /= bar_amp_c[ p ];
                        }

                        bar_amp_l[ p ] = ( bar_amp_l[ p ].max( 0.0 ) * bar_amp_p[ p ] ).min( 100.0 );
                        bar_amp_r[ p ] = ( bar_amp_r[ p ].max( 0.0 ) * bar_amp_p[ p ] ).min( 100.0 );
                    }

                    let mut bd = bar_data.lock().unwrap();

                    bd.bar_l.clear();
                    bd.bar_r.clear();

                    bd.bar_l.extend_from_slice( &bar_amp_l[ bar_st..bar_ed ] );
                    bd.bar_r.extend_from_slice( &bar_amp_r[ bar_st..bar_ed ] );
                    bd.sbuf = s_buf.len();
                    bd.fcnt += 1;

                    s_buf_delay_size = ( bd.delay.as_secs_f32() * ( SAMPLING_RATE * CHANNELS ) as f32 ) as usize;

                    tx.send( Event::Sample ).unwrap();
                }
            }
        }
    }
}


