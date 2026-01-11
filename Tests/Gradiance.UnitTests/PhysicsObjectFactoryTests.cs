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

            Assert.That(go, Is.Not.Null);
            Assert.That(go.name, Is.EqualTo("Box"));
            Assert.That((Vector2)go.transform.position, Is.EqualTo(position));

            var sr = go.GetComponent<SpriteRenderer>();
            Assert.That(sr, Is.Not.Null);
            Assert.That(sr.size, Is.EqualTo(size));
            Assert.That(sr.drawMode, Is.EqualTo(SpriteDrawMode.Sliced));

            var col = go.GetComponent<BoxCollider2D>();
            Assert.That(col, Is.Not.Null);
            Assert.That(col.size, Is.EqualTo(size));

            var rb = go.GetComponent<Rigidbody2D>();
            Assert.That(rb, Is.Not.Null);
        }

        [Test]
        public void CreateCircle_CreatesValidGameObject()
        {
            Vector2 position = new Vector2(-5, 5);
            float radius = 1.5f;

            var go = PhysicsObjectFactory.CreateCircle(position, radius);

            Assert.That(go, Is.Not.Null);
            Assert.That(go.name, Is.EqualTo("Circle"));
            Assert.That((Vector2)go.transform.position, Is.EqualTo(position));

            var sr = go.GetComponent<SpriteRenderer>();
            Assert.That(sr, Is.Not.Null);
            // Sprite size for circle is diameter
            Assert.That(sr.size, Is.EqualTo(new Vector2(3, 3)));

            var col = go.GetComponent<CircleCollider2D>();
            Assert.That(col, Is.Not.Null);
            Assert.That(col.radius, Is.EqualTo(radius));

            var rb = go.GetComponent<Rigidbody2D>();
            Assert.That(rb, Is.Not.Null);
        }
    }
}
