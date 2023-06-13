"use client";

import Image from 'next/image'
import styles from './page.module.css'
import { useEffect, useRef, useState } from 'react';
import localFont from 'next/font/local';

export const icons = localFont({
  src: '../../public/icons.ttf',
});


export default function Home() {
  const [data, setData] = useState<JSON | any>(JSON.parse("{}"));
  const [state, setState] = useState("");
  const [webSocket, setWebSocket] = useState<WebSocket | null>(null);

  const root = useRef<HTMLElement>(null);
  const seekbar = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const ws = new WebSocket(`ws://${window.location.hostname}:${Number.parseInt(window.location.port) + 1}`);

    ws.onmessage = (event) => {
      setData(JSON.parse(event.data));

      let data = JSON.parse(event.data)
      let pos = Math.floor((data.position / data.end_time) * 100);

      if (seekbar.current != null) {
        seekbar.current.style.width = `${pos}%`;
      }

      if (data.playing === true) {
        setState("");
      } else {
        setState("");
      };
    }

    setWebSocket(ws);
  }, []);

  function fullScreen() {
    if (document.fullscreenElement !== null) {
      document.exitFullscreen();
    } else {
      root.current?.requestFullscreen();
    }
  }

  function togglePlayback() {
    if (webSocket != null) {
      webSocket.send("toggle");
    }
  }

  function skip() {
    if (webSocket != null) {
      webSocket.send("skip");
    }
  }

  function back() {
    if (webSocket != null) {
      webSocket.send("back");
    }
  }

  return (
    <main className={styles.root} ref={root}>
      <div className={styles.background_container}>
        <Image src={`data:image/jpeg;base64,${data.album_artwork}`} alt="artwork" fill={true} />
      </div>
      <div className={styles.image} onClick={() => fullScreen()}>
        <div className={styles.image_container}>
          <Image src={`data:image/jpeg;base64,${data.album_artwork}`} alt="artwork" fill={true} />
        </div>
      </div>
      <div className={styles.info_container}>
        <div className={styles.info}>
          <div className={styles.main_detail}>
            <div className={styles.song_name}>{data.song_name}</div>
            <div className={styles.divider}></div>
            <div className={styles.artist}>{data.artist}</div>
            <div className={styles.album}>{data.album}</div>
          </div>
          <div className={styles.control_container}>
            <div className={styles.seek_bar}>
              <div className={styles.bar_container}>
                <div className={styles.bar} ref={seekbar}></div>
              </div>
            </div>
            <div className={`${styles.controls} ${icons.className}`}>
              <div className={styles.control} onClick={() => back()}></div>
              <div className={styles.control} onClick={() => togglePlayback()}>{state}</div>
              <div className={styles.control} onClick={() => skip()}></div>
            </div>
          </div>
        </div>
      </div>
    </main>
  )
}
