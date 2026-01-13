using System.Collections.Generic;
using UnityEngine;

namespace Core
{
    public class CommandManager : MonoBehaviour
    {
        private readonly Stack<ICommand> _undoStack = new Stack<ICommand>();
        private readonly Stack<ICommand> _redoStack = new Stack<ICommand>();

        public static CommandManager Instance { get; private set; }

        private void Awake()
        {
            if (Instance != null && Instance != this)
            {
                Destroy(gameObject);
                return;
            }

            Instance = this;
        }

        public void ExecuteCommand(ICommand command)
        {
            command.Execute();
            _undoStack.Push(command);
            _redoStack.Clear(); // Clearing redo stack when a new action is performed
        }

        public void Undo()
        {
            if (_undoStack.Count > 0)
            {
                var command = _undoStack.Pop();
                command.Undo();
                _redoStack.Push(command);
            }
        }

        public void Redo()
        {
            if (_redoStack.Count > 0)
            {
                var command = _redoStack.Pop();
                command.Execute();
                _undoStack.Push(command);
            }
        }

        private void Update()
        {
            // Keyboard shortcuts
            if (Input.GetKeyDown(KeyCode.Z) && (Input.GetKey(KeyCode.LeftControl) || Input.GetKey(KeyCode.RightControl)))
            {
                Undo();
            }

            if (Input.GetKeyDown(KeyCode.Y) && (Input.GetKey(KeyCode.LeftControl) || Input.GetKey(KeyCode.RightControl)))
            {
                Redo();
            }
        }
    }
}
