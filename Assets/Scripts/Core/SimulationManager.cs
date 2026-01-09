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

        // TODO: [Phase 2] Implement Undo/Redo System
        // - Create a Command interface (ICommand) with Execute() and Undo() methods.
        // - Maintain a Stack<ICommand> for undo history and a Stack<ICommand> for redo history.
        // - Actions like CreateObject, DeleteObject, MoveObject, ChangeProperty should be encapsulated as Commands.

        // TODO: [Phase 2] Implement Scene Serialization
        // - Add methods to Serialize current scene state to JSON.
        // - Add methods to Deserialize and reconstruct the scene.
        // - Ensure all PhysicsObjects have unique IDs for reference serialization.
    }
}
