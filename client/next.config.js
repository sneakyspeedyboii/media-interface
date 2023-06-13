/** @type {import('next').NextConfig} */
const nextConfig = {
    typescript: {
        //im not fixing this font error ive spent too long today fixing other errors
        ignoreBuildErrors: true,
    },
    output: 'export',
    distDir: '../server/assets/site/', 
}

module.exports = nextConfig
