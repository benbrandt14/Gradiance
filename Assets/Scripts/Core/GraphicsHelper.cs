using UnityEngine;

namespace Core
{
    public static class GraphicsHelper
    {
        public static Sprite CreateCircleSprite(int res = 32, Color? color = null)
        {
            var tex = new Texture2D(res, res);
            var cols = new Color[res * res];
            float c = res / 2f;
            float rSq = (c - 1) * (c - 1);
            Color drawColor = color ?? Color.white;

            for (int y = 0; y < res; y++)
            {
                for (int x = 0; x < res; x++)
                {
                    float d = ((x - c) * (x - c)) + ((y - c) * (y - c));
                    cols[(y * res) + x] = (d < rSq) ? drawColor : Color.clear;
                }
            }

            tex.SetPixels(cols);
            tex.Apply();
            return Sprite.Create(tex, new Rect(0, 0, res, res), new Vector2(0.5f, 0.5f), res);
        }
    }
}
