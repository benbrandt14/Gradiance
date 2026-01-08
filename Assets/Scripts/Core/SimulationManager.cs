using UnityEngine;

namespace Core
{
    public class SimulationManager : MonoBehaviour
    {
        public static SimulationManager Instance { get; private set; }

        public bool IsPaused { get; private set; } = false;

        private float _timeScaleBeforePause = 1.0f;

        private void Awake()
        {
            if (Instance == null)
            {
                Instance = this;
            }
            else
            {
                Destroy(gameObject);
            }
        }

        public void SetPaused(bool paused)
        {
            IsPaused = paused;
            if (IsPaused)
            {
                _timeScaleBeforePause = Time.timeScale;
                Time.timeScale = 0f;
            }
            else
            {
                Time.timeScale = _timeScaleBeforePause > 0 ? _timeScaleBeforePause : 1.0f;
            }
        }

        public void TogglePause()
        {
            SetPaused(!IsPaused);
        }

        public void SetGravity(Vector2 gravity)
        {
            Physics2D.gravity = gravity;
        }
    }
}
