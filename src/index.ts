import * as UPNG from 'upng-js';

export interface Env {}

export default {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    const url = new URL(request.url);

    const imageUrl = url.searchParams.get('img');
    if (!imageUrl) {
      return new Response('Missing "img" query parameter.', { status: 400 });
    }

    try {
      const response = await fetch(imageUrl);
      if (!response.ok) {
        return new Response('Failed to fetch the target image.', { status: 502 });
      }

      const arrayBuffer = await response.arrayBuffer();

      const img = UPNG.decode(arrayBuffer);
      const width = img.width;
      const height = img.height;

      const rgbaBuffer = new Uint8Array(UPNG.toRGBA8(img)[0]);

      const scale = width / 512;

      const queryStrip = url.searchParams.get('strip');
      const queryRadius = url.searchParams.get('radius');
      const stripHeight = Math.round(queryStrip ? Number(queryStrip) : (17 * scale));
      const radius = Math.round(queryRadius ? Number(queryRadius) : (36 * scale));

      for (let y = 0; y < stripHeight; y++) {
        for (let x = 0; x < width; x++) {
          const idx = (y * width + x) * 4;
          rgbaBuffer[idx + 3] = 0;
        }
      }

      const centerX = width - radius;
      const centerY = stripHeight + radius;

      for (let y = stripHeight; y < centerY; y++) {
        for (let x = centerX; x < width; x++) {
          const dx = x - centerX;
          const dy = y - centerY;

          if ((dx * dx) + (dy * dy) > (radius * radius)) {
            const idx = (y * width + x) * 4;
            rgbaBuffer[idx + 3] = 0;
          }
        }
      }

      const outputPng = UPNG.encode([rgbaBuffer.buffer], width, height, 0);

      return new Response(outputPng, {
        headers: {
          'Content-Type': 'image/png',
          'Cache-Control': 'public, max-age=2592000',
          'Access-Control-Allow-Origin': '*'
        }
      });

    } catch (error: any) {
      return new Response(`Error processing image: ${error.message || 'Unknown error'}`, { status: 500 });
    }
  }
} satisfies ExportedHandler<Env>;
