use std::io::{ Write, Stdout };

use termion::*;
use termion::raw::{ IntoRawMode, RawTerminal };

mod colorex;
mod backtask;
mod render;

fn main()
{
    env_logger::init();

    let disable_output = false;

    let mut stdout : Option< RawTerminal< Stdout > > = None;

    if !disable_output
    {
        stdout = Some( std::io::stdout().into_raw_mode().unwrap() );
        write!( stdout.as_mut().unwrap(), "{}{}", clear::All, cursor::Hide).unwrap();
        stdout.as_mut().unwrap().flush().unwrap();
    }
    else
    {
        log::debug!( "start" );
    }

    let backtask = backtask::BackTask::new( 65 );

    loop
    {
        match backtask.next().unwrap()
        {
            backtask::Event::Tick =>
            {
                let text =
                {
                    let bd = backtask.bar_data.lock().unwrap();
                    render::render( &bd )
                };

                if !disable_output
                {
                    write!( stdout.as_mut().unwrap(), "{}{}", clear::All, text ).unwrap();
                    stdout.as_mut().unwrap().flush().unwrap();
                }
            }

        ,   backtask::Event::Exit =>
            {
                break;
            }
        ,   backtask::Event::Sample =>
            {

            }
        }
    }

    if !disable_output
    {
        write!( stdout.as_mut().unwrap(), "{}", clear::All ).unwrap();
        write!( stdout.as_mut().unwrap(), "{}", termion::cursor::Show ).unwrap();
        stdout.as_mut().unwrap().flush().unwrap();
    }
    else
    {
        log::debug!( "stop" );
    }
}
