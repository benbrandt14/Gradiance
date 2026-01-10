using NUnit.Framework;
using UnityEngine;
using Physics;

namespace Gradiance.UnitTests
{
    public class PhysicsObjectFactoryTests
    {
        [Test]
        public void CreateBox_CreatesValidGameObject()
        {
            Vector2 position = new Vector2(10, 20);
            Vector2 size = new Vector2(2, 3);

            var go = PhysicsObjectFactory.CreateBox(position, size);

            Assert.IsNotNull(go);
            Assert.AreEqual("Box", go.name);
            Assert.AreEqual(position, (Vector2)go.transform.position);

            var sr = go.GetComponent<SpriteRenderer>();
            Assert.IsNotNull(sr);
            Assert.AreEqual(size, sr.size);
            Assert.AreEqual(SpriteDrawMode.Sliced, sr.drawMode);

            var col = go.GetComponent<BoxCollider2D>();
            Assert.IsNotNull(col);
            Assert.AreEqual(size, col.size);

            var rb = go.GetComponent<Rigidbody2D>();
            Assert.IsNotNull(rb);
        }

        [Test]
        public void CreateCircle_CreatesValidGameObject()
        {
            Vector2 position = new Vector2(-5, 5);
            float radius = 1.5f;

            var go = PhysicsObjectFactory.CreateCircle(position, radius);

            Assert.IsNotNull(go);
            Assert.AreEqual("Circle", go.name);
            Assert.AreEqual(position, (Vector2)go.transform.position);

            var sr = go.GetComponent<SpriteRenderer>();
            Assert.IsNotNull(sr);
            // Sprite size for circle is diameter
            Assert.AreEqual(new Vector2(3, 3), sr.size);

            var col = go.GetComponent<CircleCollider2D>();
            Assert.IsNotNull(col);
            Assert.AreEqual(radius, col.radius);

            var rb = go.GetComponent<Rigidbody2D>();
            Assert.IsNotNull(rb);
        }
    }
}
