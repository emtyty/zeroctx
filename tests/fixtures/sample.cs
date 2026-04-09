using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;

namespace SampleApp.Services
{
    public interface IUserRepository
    {
        Task<User> GetByIdAsync(int id);
        Task<IEnumerable<User>> GetAllAsync();
        Task<bool> DeleteAsync(int id);
    }

    public class UserService
    {
        private readonly IUserRepository _repo;
        private readonly ILogger _logger;

        public UserService(IUserRepository repo, ILogger logger)
        {
            _repo = repo ?? throw new ArgumentNullException(nameof(repo));
            _logger = logger;
        }

        public async Task<User> GetUserAsync(int id)
        {
            _logger.LogInformation($"Getting user {id}");
            var user = await _repo.GetByIdAsync(id);
            if (user == null)
                throw new KeyNotFoundException($"User {id} not found");
            return user;
        }

        public async Task<List<User>> SearchUsersAsync(string query)
        {
            var all = await _repo.GetAllAsync();
            return all.Where(u => u.Name.Contains(query, StringComparison.OrdinalIgnoreCase))
                      .ToList();
        }

        private void ValidateUser(User user)
        {
            if (string.IsNullOrEmpty(user.Name))
                throw new ArgumentException("Name required");
            if (user.Age < 0 || user.Age > 150)
                throw new ArgumentOutOfRangeException(nameof(user.Age));
        }
    }

    public class User
    {
        public int Id { get; set; }
        public string Name { get; set; }
        public int Age { get; set; }
        public string Email { get; set; }
    }

    public enum UserRole
    {
        Admin,
        User,
        Guest
    }
}
// line 68
// line 69
// line 70
// line 71
// line 72
// line 73
// line 74
// line 75
// line 76
// line 77
// line 78
// line 79
// line 80
// line 81
// line 82
// line 83
// line 84
// line 85
