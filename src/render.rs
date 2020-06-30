use termion::*;

use crate::backtask::BarData;
use crate::colorex;

pub fn render( bd : &BarData ) -> String
{
    let mut ret = String::new();

    let ( wx, wy ) = termion::terminal_size().unwrap();

    ret += &format!( "{} sbuf:{:6} / fcnt:{:6} / rcnt:{:6} / scnt:{:6} / delay:{:?} (PageUp/PageDown)"
            , cursor::Goto( 1, 1 )
            , bd.sbuf
            , bd.fcnt
            , bd.rcnt
            , bd.scnt
            , bd.delay
        );

    let mut bar_h_cap = Vec::< String >::new();

    for &x in &bd.bar_h
    {
        if x < 1024
        {
            bar_h_cap.push( format!( "-{:4} ", x ) );
        }
        else
        {
            bar_h_cap.push( format!( "-{:4.1}k", x as f32/ 1024.0 ));
        }
    }

    let cap_max = bar_h_cap.iter().fold( 0, | m, x | { m.max( x.len() ) } ) as u16;

    for x in 0..bar_h_cap.len()
    {
        for y in 0..bar_h_cap[ x ].len()
        {
            ret += &format!( "{}{}"
                , cursor::Goto( ( x + 1 ) as u16, ( wy - cap_max ) + ( y + 1 ) as u16 )
                , bar_h_cap[ x ].chars().nth( y ).unwrap()
            );

            ret += &format!( "{}{}"
                , cursor::Goto( ( bar_h_cap.len() * 2 - x + 1 ) as u16, ( wy - cap_max ) + ( y + 1 ) as u16 )
                , bar_h_cap[ x ].chars().nth( y ).unwrap()
            );
        }
    }

    let wwyt : u16 = 2;
    let wwyb : u16 = ( wy - cap_max ) ;
    let wwyh : u16 = wwyb - wwyt;
    let ampmax : f32 = 100.0;

    for ( x, &v ) in bd.bar_l.iter().enumerate()
    {
        let v : u16 = ( ( v.min( ampmax ).max( 0.0 ) / ampmax ) * wwyh as f32 ) as u16;

        for y in 0..v
        {
            ret += &format!( "{}{}{}{}"
                , cursor::Goto( ( x + 1 ) as u16, wwyb - y as u16 )
                , color::Fg( colorex::Magenta )
                , "="
                , style::Reset
            );
        }
    }

    for ( x, &v ) in bd.bar_r.iter().enumerate()
    {
        let v : u16 = ( ( v.min( ampmax ) / ampmax ) * wwyh as f32 ) as u16;

        for y in 0..v
        {
            ret += &format!( "{}{}{}{}"
                , cursor::Goto( ( bar_h_cap.len() * 2 - x + 1 ) as u16, wwyb - y as u16 )
                , color::Fg( colorex::Cyan )
                , "="
                , style::Reset
            );
        }
    }

    return ret;
}
