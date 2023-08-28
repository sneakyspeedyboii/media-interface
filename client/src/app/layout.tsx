import './globals.css'
import { Quicksand } from 'next/font/google'

import localFont from 'next/font/local'

export const metadata = {
  title: 'Media Interface',
  description: 'cheeky ol media interface',
}

const icons = localFont({
  src: '../../public/icons.ttf',
  variable: '--icons-font-var'
});

const quick_sand = Quicksand({
  subsets: ['latin'],
  variable: '--main-font-var'
})


export default function RootLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return (
    <html lang="en" className={`${quick_sand.variable} ${icons.variable}`}>
      <body>{children}</body>
    </html>
  )
}
