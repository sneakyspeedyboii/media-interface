"use client"

import Image from 'next/image'
import { useEffect, useRef, useState } from 'react';

const layout = { song_name: "Loading", song_subtitle: "Loading", artist: "Loading", album: "Loading", start_time: 0, end_time: 2, position: 1, playing: true, album_artwork: "" }

export default function Home() {
  const [data, setData] = useState(layout);
  const [screenHeight, setScreenHeight] = useState(0);
  const [screenWidth, setScreenWidth] = useState(0);
  const [webSocket, setWebSocket] = useState<WebSocket | null>(null);

  const seek_bar_ref = useRef<HTMLDivElement>(null);
  const root_ref = useRef<HTMLDivElement>(null);

  //Could probably trim down the use of useEffect. Im too lazy to do that

  useEffect(() => {
    const ws = new WebSocket(`ws://${window.location.hostname}:${Number.parseInt(window.location.port) + 1}`);
    setWebSocket(ws);

    ws.onmessage = (event) => {
      setData(JSON.parse(event.data));
    }

    return () => {
      ws.close();
    }
  }, []);

  useEffect(() => {
    if (seek_bar_ref.current != null) {
      seek_bar_ref.current.style.width = `${(data.position / data.end_time) * 100}%`;
    }
  }, [data])

  useEffect(() => {
    setScreenHeight(window.innerHeight);
    setScreenWidth(window.innerWidth);
  }, [])

  useEffect(() => {
    function onResize() {
      if (typeof window !== 'undefined') {
        setScreenHeight(window.innerHeight);
        setScreenWidth(window.innerWidth);
      };
    }

    window.addEventListener('resize', onResize);
    return () => {
      window.removeEventListener('resize', onResize);
    }
  }, [screenHeight, screenWidth]);

  return (
    <main className='bg-black' ref={root_ref}>
      <div className='absolute top-0 left-0 w-screen h-screen min-h-screen overflow-hidden'>
        <div className={`absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 aspect-square blur-xl brightness-90 ${screenWidth > screenHeight ? "w-[102vw]" : "h-[102vh]"}`}>
          <Image src={`data:image/jpeg;base64,${data.album_artwork}`} alt="artwork" fill={true} />
        </div>
      </div>

      <div className='w-screen h-screen min-h-screen flex items-center justify-evenly'>
        <div className='h-full w-1/2 flex items-center justify-center'>
          <div className='relative aspect-square w-8/12 rounded-xl overflow-hidden drop-shadow-3xl '>
            <Image onClick={() => {
              if (document.fullscreenElement != null) {
                document.exitFullscreen();
              } else {
                if (root_ref != null) {
                  root_ref.current?.requestFullscreen();
                }
              }
            }} src={`data:image/jpeg;base64,${data.album_artwork}`} alt="artwork" fill={true} />
          </div>
        </div>

        <div className='h-full w-1/2 flex items-center justify-center z-10'>

          <div className='w-10/12 h-full flex flex-col items-center justify-evenly'>

            <div className='h-4/6 w-full flex flex-col items-center justify-evenly font-main text-white'>
              <div className='w-full text-4xl font-bold'>{data.song_name}</div>
              <div className='content-[""] bg-white w-full h-1 rounded-full'></div>
              <div className='w-full text-2xl font-semibold'>{data.artist}</div>
              <div className='w-full text-2xl font-semibold'>{data.album}</div>
            </div>

            <div className='w-full h-2/6 flex flex-col items-center justify-evenly'>

              <div className='w-full'>
                <div className='content-[""] bg-gray-500 w-full h-1 rounded-full relative'>
                  <div ref={seek_bar_ref} className={`absolute content-[""] h-1 top-0 rounded-full bg-white`}></div>
                </div>
              </div>
              <div className='w-full flex items-center justify-evenly font-icons text-white text-4xl'>
                <div onClick={() => {
                  if (webSocket != null) {
                    webSocket.send("back");
                  }
                }}></div>
                <div onClick={() => {
                  if (webSocket != null) {
                    webSocket.send("toggle");
                  }
                }}>{data.playing ? "" : ""}</div>
                <div onClick={() => {
                  if (webSocket != null) {
                    webSocket.send("skip");
                  }
                }}></div>
              </div>

            </div>
          </div>

        </div>
      </div>
    </main>
  )
}